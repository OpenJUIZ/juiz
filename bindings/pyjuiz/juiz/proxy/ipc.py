"""IPC ブローカーのクライアント (issue #236、docs/spec/ipc-broker.md §5.2・large-data.md §3)

同じマシンの juiz に、Unix ドメインソケット (Windows は名前付きパイプ) で繋ぐ。
要求と応答は JZF1 のフレームで、画像は PNG にせず画素のまま、要素の多い数の配列は生バイトで送る。

- 画像を送るときは PIL の Image (L / LA / RGB / RGBA / I;16) か、numpy の配列 (uint8・uint16・float32) を渡す。
- 受け取った画像は、PIL で表せる形式 (l8 / la8 / rgb8 / rgba8 / l16) なら PIL の Image、
  それ以外 (la16 / rgb16 / rgba16 / rgb32f / rgba32f) は numpy の配列 (高さ × 幅 × チャンネル) になる。
- 接続は最初の要求で作って使い回す。要求は 1 つずつ送る。
"""
import array
import json
import os
import re
import socket
import struct
import sys
import threading

from PIL import Image

MAGIC = b'JZF1'

DEFAULT_IPC_NAME = 'juiz'
IPC_DIR_ENV = 'JUIZ_IPC_DIR'

# 応答の待ち時間の既定値 [秒] (Rust の IpcBrokerProxy と同じ 10 秒)
DEFAULT_TIMEOUT = 10.0

# 1 フレームの合計の上限の既定値 (1 GiB) と、ヘッダの上限 (64 MiB)
DEFAULT_MAX_FRAME_BYTES = 1 << 30
MAX_HEADER_BYTES = 64 << 20

# 数の配列を本体に出す要素の数の下限 (large-data.md §3.4)
MIN_ARRAY_BODY_COUNT = 1024

_NAME_REGEX = re.compile(r'^[A-Za-z0-9_.\-]{1,64}$')

_I64_MIN = -(1 << 63)
_I64_MAX = (1 << 63) - 1

# format: (チャンネルの数, 1 チャンネルのバイト数, numpy の dtype)
_IMAGE_FORMATS = {
    'l8': (1, 1, 'uint8'),
    'la8': (2, 1, 'uint8'),
    'rgb8': (3, 1, 'uint8'),
    'rgba8': (4, 1, 'uint8'),
    'l16': (1, 2, 'uint16'),
    'la16': (2, 2, 'uint16'),
    'rgb16': (3, 2, 'uint16'),
    'rgba16': (4, 2, 'uint16'),
    'rgb32f': (3, 4, 'float32'),
    'rgba32f': (4, 4, 'float32'),
}

# PIL のモードと format の対応 (PIL で表せるものだけ)
_PIL_MODE_TO_FORMAT = {'L': 'l8', 'LA': 'la8', 'RGB': 'rgb8', 'RGBA': 'rgba8', 'I;16': 'l16'}
_FORMAT_TO_PIL_MODE = {v: k for k, v in _PIL_MODE_TO_FORMAT.items()}

# numpy の (dtype, チャンネルの数) と format の対応
_NUMPY_TO_FORMAT = {
    ('uint8', 1): 'l8', ('uint8', 2): 'la8', ('uint8', 3): 'rgb8', ('uint8', 4): 'rgba8',
    ('uint16', 1): 'l16', ('uint16', 2): 'la16', ('uint16', 3): 'rgb16', ('uint16', 4): 'rgba16',
    ('float32', 3): 'rgb32f', ('float32', 4): 'rgba32f',
}

_LITTLE = sys.byteorder == 'little'


class IpcError(Exception):
    """フレームが読めない、接続が切れたなど、IPC の通り道の失敗"""
    pass


def validate_ipc_name(name: str):
    """名前は [A-Za-z0-9_-] と . からなる 1〜64 文字で、. で始まらない (ipc-broker.md §2.1)"""
    if not isinstance(name, str) or not _NAME_REGEX.match(name) or name.startswith('.'):
        raise ValueError(f"invalid ipc broker name '{name}'. "
                         "Use 1 to 64 characters of [A-Za-z0-9_-.], not starting with '.'")


def ipc_socket_dir() -> str:
    """ソケットを置くディレクトリ (ipc-broker.md §2.2)。クライアントなので作らない"""
    d = os.environ.get(IPC_DIR_ENV)
    if d:
        return d
    runtime = os.environ.get('XDG_RUNTIME_DIR')
    if runtime:
        return os.path.join(runtime, 'juiz')
    return f'/tmp/juiz-{os.getuid()}'


def ipc_address(name: str) -> str:
    """名前のソケットのパス (Unix) か名前付きパイプの名前 (Windows)"""
    validate_ipc_name(name)
    if os.name == 'nt':
        return rf'\\.\pipe\juiz-{name}'
    return os.path.join(ipc_socket_dir(), f'{name}.sock')


# ---- 本体 (large-data.md §3.4) ----

def _numpy():
    try:
        import numpy
        return numpy
    except ImportError:
        return None


def _is_ndarray(x):
    np = _numpy()
    return np is not None and isinstance(x, np.ndarray)


def _image_body(x):
    """画像なら (本体の説明, 本体のバイト列) を返す。画像でなければ None"""
    if isinstance(x, Image.Image):
        fmt = _PIL_MODE_TO_FORMAT.get(x.mode)
        if fmt is None:
            # P (パレット) などは RGB / RGBA にしてから送る
            x = x.convert('RGBA' if 'A' in x.getbands() or 'transparency' in x.info else 'RGB')
            fmt = _PIL_MODE_TO_FORMAT[x.mode]
        channels, channel_bytes, _ = _IMAGE_FORMATS[fmt]
        data = x.tobytes()
        if fmt == 'l16' and not _LITTLE:
            data = x.tobytes('raw', 'I;16')
        w, h = x.size
        return ({'kind': 'image', 'width': w, 'height': h, 'format': fmt,
                 'stride': w * channels * channel_bytes}, data)
    if _is_ndarray(x):
        np = _numpy()
        if x.ndim not in (2, 3):
            return None
        channels = 1 if x.ndim == 2 else x.shape[2]
        fmt = _NUMPY_TO_FORMAT.get((x.dtype.name, channels))
        if fmt is None:
            raise ValueError(f'this numpy array can not be sent as an image '
                             f'(dtype {x.dtype.name}, shape {x.shape})')
        _, channel_bytes, dtype = _IMAGE_FORMATS[fmt]
        a = np.ascontiguousarray(x, dtype=np.dtype(dtype).newbyteorder('<'))
        h, w = a.shape[0], a.shape[1]
        return ({'kind': 'image', 'width': w, 'height': h, 'format': fmt,
                 'stride': w * channels * channel_bytes}, a.data)
    return None


def _array_body(v):
    """要素の多い数の配列なら (本体の説明, 本体) を返す。整数と小数が混ざっていれば None"""
    if not isinstance(v, list) or len(v) < MIN_ARRAY_BODY_COUNT:
        return None
    # bool は int の一種だが、JSON では真偽値なので配列の本体にしない
    if all(type(e) is int and _I64_MIN <= e <= _I64_MAX for e in v):
        a = array.array('q', v)
        dtype = 'i64le'
    elif all(type(e) is float for e in v):
        a = array.array('d', v)
        dtype = 'f64le'
    else:
        return None
    if not _LITTLE:
        a.byteswap()
    return ({'kind': 'array', 'dtype': dtype, 'count': len(v)}, a.tobytes())


def _read_image(desc, data: bytes):
    fmt = desc.get('format')
    if fmt not in _IMAGE_FORMATS:
        raise IpcError(f'unknown image format in frame: {fmt}')
    channels, channel_bytes, dtype = _IMAGE_FORMATS[fmt]
    w, h, stride = desc['width'], desc['height'], desc['stride']
    row = w * channels * channel_bytes
    if stride < row or len(data) != stride * h:
        raise IpcError(f'invalid image body ({desc}, {len(data)} bytes)')
    if stride != row:
        data = b''.join(data[y * stride:y * stride + row] for y in range(h))
    mode = _FORMAT_TO_PIL_MODE.get(fmt)
    if mode is not None:
        return Image.frombytes(mode, (w, h), data, 'raw', mode)
    np = _numpy()
    if np is None:
        raise IpcError(f'receiving an image of format {fmt} needs numpy')
    a = np.frombuffer(data, dtype=np.dtype(dtype).newbyteorder('<')).astype(dtype, copy=False)
    return a.reshape((h, w, channels)) if channels > 1 else a.reshape((h, w))


def _read_array(desc, data: bytes):
    dtype, count = desc.get('dtype'), desc.get('count')
    if dtype == 'i64le':
        a = array.array('q')
    elif dtype == 'f64le':
        a = array.array('d')
    else:
        raise IpcError(f'unknown array dtype in frame: {dtype}')
    if len(data) != count * 8:
        raise IpcError(f'invalid array body ({desc}, {len(data)} bytes)')
    a.frombytes(data)
    if not _LITTLE:
        a.byteswap()
    return a.tolist()


# ---- フレーム (large-data.md §3.1・§3.2) ----

class _OutgoingFrame:
    def __init__(self, id, kind):
        self.header = {'id': id, 'kind': kind}
        self.descs = []
        self.bodies = []

    def capsule_repr(self, x):
        """値をカプセルの表し方 (§3.3) にする。本体に出すものは bodies に足す"""
        if x is None:
            return {'value': None}
        body = _image_body(x)
        if body is None:
            body = _array_body(x)
        if body is not None:
            self.descs.append(body[0])
            self.bodies.append(body[1])
            return {'body': len(self.bodies) - 1}
        return {'value': x}

    def to_buffers(self):
        if self.descs:
            self.header['bodies'] = self.descs
        header = json.dumps(self.header, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
        small = [MAGIC, struct.pack('<I', len(header)), header]
        out = [b''.join(small)]
        for body in self.bodies:
            out.append(struct.pack('<Q', len(memoryview(body).cast('B'))))
            out.append(body)
        return out


def request_frame(id, param: dict, map: dict):
    f = _OutgoingFrame(id, 'request')
    f.header['param'] = {k: str(v) for k, v in param.items()}
    f.header['map'] = {k: f.capsule_repr(v) for k, v in map.items()}
    return f


class _Reader:
    """ソケット (かパイプ) から決まった長さを読む"""

    def __init__(self, recv_into):
        self._recv_into = recv_into

    def read_exact(self, n):
        buf = bytearray(n)
        view = memoryview(buf)
        got = 0
        while got < n:
            k = self._recv_into(view[got:])
            if not k:
                raise EOFError('the server closed the connection')
            got += k
        return bytes(buf)


def read_frame(reader: _Reader, max_frame_bytes=DEFAULT_MAX_FRAME_BYTES):
    """フレームを 1 つ読み、(ヘッダ, 本体のリスト) を返す"""
    magic = reader.read_exact(4)
    if magic != MAGIC:
        raise IpcError(f'unknown frame magic {magic!r}')
    (header_len,) = struct.unpack('<I', reader.read_exact(4))
    if header_len > MAX_HEADER_BYTES:
        raise IpcError(f'frame header is too large ({header_len} bytes)')
    header = json.loads(reader.read_exact(header_len).decode('utf-8'))
    total = header_len
    bodies = []
    for desc in header.get('bodies', []):
        (body_len,) = struct.unpack('<Q', reader.read_exact(8))
        total += body_len
        if total > max_frame_bytes:
            raise IpcError(f'frame is too large (> {max_frame_bytes} bytes)')
        data = reader.read_exact(body_len) if body_len else b''
        if desc.get('shm') is not None:
            raise IpcError('a body in shared memory can not be read by pyjuiz')
        kind = desc.get('kind')
        if kind == 'image':
            bodies.append(_read_image(desc, data))
        elif kind == 'array':
            bodies.append(_read_array(desc, data))
        else:
            raise IpcError(f'unknown body kind in frame: {kind}')
    return header, bodies


def capsule_from_repr(repr, bodies):
    if not isinstance(repr, dict):
        raise IpcError(f'invalid capsule in frame ({repr!r})')
    if 'value' in repr:
        return repr['value']
    if repr.get('empty') is True:
        return None
    if 'body' in repr:
        index = repr['body']
        if not isinstance(index, int) or not 0 <= index < len(bodies):
            raise IpcError(f'body {index} is not in the frame')
        return bodies[index]
    raise IpcError(f'invalid capsule in frame ({repr!r})')


# ---- 通り道 ----

class _UnixConnection:
    def __init__(self, path, timeout):
        self._timeout = timeout
        self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._sock.settimeout(timeout)
        try:
            self._sock.connect(path)
        except OSError:
            self._sock.close()
            raise
        self.reader = _Reader(self._sock.recv_into)

    def write(self, buffers):
        # 本体は複製せずに書く
        if hasattr(self._sock, 'sendmsg'):
            views = [memoryview(b).cast('B') for b in buffers]
            while views:
                sent = self._sock.sendmsg(views)
                while views and sent >= len(views[0]):
                    sent -= len(views[0])
                    views.pop(0)
                if views and sent:
                    views[0] = views[0][sent:]
        else:
            for b in buffers:
                self._sock.sendall(b)

    def is_closed(self):
        """相手が閉じていれば True (ノンブロッキングで覗いて EOF を見る)"""
        try:
            self._sock.setblocking(False)
            try:
                return self._sock.recv(1, socket.MSG_PEEK) == b''
            finally:
                self._sock.settimeout(self._timeout)
        except BlockingIOError:
            return False
        except OSError:
            return True

    def close(self):
        try:
            self._sock.close()
        except OSError:
            pass


class _PipeConnection:
    """Windows の名前付きパイプ。読み取りのタイムアウトは無い (Rust の IpcBrokerProxy と同じ制限)"""

    def __init__(self, path, timeout):
        self._f = open(path, 'r+b', buffering=0)
        self.reader = _Reader(self._f.readinto)

    def write(self, buffers):
        for b in buffers:
            view = memoryview(b).cast('B')
            while view:
                n = self._f.write(view)
                view = view[n:]

    def is_closed(self):
        return self._f.closed

    def close(self):
        try:
            self._f.close()
        except OSError:
            pass


def _connect(address, timeout):
    if os.name == 'nt':
        return _PipeConnection(address, timeout)
    return _UnixConnection(address, timeout)
