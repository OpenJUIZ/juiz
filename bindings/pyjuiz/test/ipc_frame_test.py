"""pyjuiz の IPC クライアントのフレーム (JZF1、docs/spec/large-data.md §3) のテスト (issue #236)

サーバーは使わない。bindings/pyjuiz で実行する:

    python test/ipc_frame_test.py

numpy があれば numpy の配列の画像も確かめる。
"""
import json
import os
import socket
import struct
import sys
import threading

sys.path.append('.')

from PIL import Image

from juiz.proxy import IpcProxy, IpcRemoteError, IpcError, ipc_address
from juiz.proxy import ipc

try:
    import numpy as np
except ImportError:
    np = None


class _BytesReader:
    def __init__(self, data):
        self._data = memoryview(data)
        self._pos = 0

    def recv_into(self, view):
        n = min(len(view), len(self._data) - self._pos)
        view[:n] = self._data[self._pos:self._pos + n]
        self._pos += n
        return n


def _bytes_of(frame):
    return b''.join(bytes(memoryview(b).cast('B')) for b in frame.to_buffers())


def _roundtrip(map):
    data = _bytes_of(ipc.request_frame(7, {'method_name': 'update'}, map))
    header, bodies = ipc.read_frame(ipc._Reader(_BytesReader(data).recv_into))
    assert header['id'] == 7 and header['kind'] == 'request'
    return header, {k: ipc.capsule_from_repr(r, bodies) for k, r in header['map'].items()}


def test_values_and_arrays():
    ints = list(range(2000))
    floats = [i / 3 for i in range(1500)]
    header, got = _roundtrip({'a': 1, 'b': 2.0, 'c': {'x': [1, 'y']}, 'd': None,
                              'ints': ints, 'floats': floats, 'small': [1, 2, 3],
                              'mixed': [1, 2.5] * 600, 'bools': [True] * 1200})
    assert got['a'] == 1 and type(got['a']) is int
    assert got['b'] == 2.0 and type(got['b']) is float
    assert got['c'] == {'x': [1, 'y']} and got['d'] is None
    assert got['ints'] == ints and got['floats'] == floats
    assert got['mixed'] == [1, 2.5] * 600 and got['bools'] == [True] * 1200
    kinds = sorted((d['dtype'], d['count']) for d in header['bodies'])
    # 本体になるのは 1024 要素以上の整数だけ・小数だけの配列
    assert kinds == [('f64le', 1500), ('i64le', 2000)], kinds
    assert header['map']['small'] == {'value': [1, 2, 3]}
    assert header['map']['bools'] == {'value': [True] * 1200}


def test_images():
    img = Image.new('RGB', (31, 17))
    img.putpixel((30, 16), (1, 2, 3))
    header, got = _roundtrip({'img': img})
    assert header['bodies'] == [{'kind': 'image', 'width': 31, 'height': 17, 'format': 'rgb8', 'stride': 93}]
    assert got['img'].mode == 'RGB' and got['img'].tobytes() == img.tobytes()
    l16 = Image.frombytes('I;16', (4, 2), struct.pack('<8H', 0, 1, 256, 65535, 7, 8, 9, 10))
    _, got = _roundtrip({'img': l16})
    assert got['img'].getpixel((3, 0)) == 65535 and got['img'].getpixel((1, 0)) == 1


def test_padded_stride():
    # 行の終わりに余白のある画像 (C++ の cv::Mat など) を読める
    header = {'id': 1, 'kind': 'response', 'result': {'body': 0},
              'bodies': [{'kind': 'image', 'width': 2, 'height': 2, 'format': 'l8', 'stride': 4}]}
    h = json.dumps(header).encode()
    data = b'JZF1' + struct.pack('<I', len(h)) + h + struct.pack('<Q', 8) + bytes([1, 2, 99, 99, 3, 4, 99, 99])
    header, bodies = ipc.read_frame(ipc._Reader(_BytesReader(data).recv_into))
    img = ipc.capsule_from_repr(header['result'], bodies)
    assert img.tobytes() == bytes([1, 2, 3, 4])


def test_bad_frames():
    try:
        ipc.read_frame(ipc._Reader(_BytesReader(b'XXXX\0\0\0\0').recv_into))
        raise AssertionError('an unknown magic must fail')
    except ipc.IpcError:
        pass
    try:
        ipc.read_frame(ipc._Reader(_BytesReader(b'JZF1').recv_into))
        raise AssertionError('a truncated frame must fail')
    except EOFError:
        pass
    for name in ['', '.hidden', 'a/b', 'x' * 65]:
        try:
            ipc.validate_ipc_name(name)
            raise AssertionError(f'{name!r} must be invalid')
        except ValueError:
            pass


def test_numpy():
    if np is None:
        print('numpy is not installed; skipping numpy tests')
        return
    for arr, fmt in [(np.zeros((3, 5), np.uint8), 'l8'), (np.ones((3, 5, 3), np.uint16), 'rgb16'),
                     (np.full((3, 5, 4), 0.5, np.float32), 'rgba32f')]:
        header, got = _roundtrip({'img': arr})
        assert header['bodies'][0]['format'] == fmt
        assert np.array_equal(np.asarray(got['img']), arr), fmt
    try:
        _roundtrip({'img': np.zeros((2, 2, 3), np.int32)})
        raise AssertionError('int32 images must fail')
    except ValueError:
        pass


def _fake_server(path, responses):
    """要求を 1 つ読むたびに responses の次の (kind, 中身) を返す Unix ソケットのサーバー"""
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(path)
    srv.listen(1)

    def run():
        conn, _ = srv.accept()
        reader = ipc._Reader(conn.recv_into)
        for kind, body in responses:
            header, _ = ipc.read_frame(reader)
            out = ipc._OutgoingFrame(header['id'], kind)
            if kind == 'error':
                out.header['error'] = body
            else:
                out.header['result'] = out.capsule_repr(body)
            for b in out.to_buffers():
                conn.sendall(b)
        conn.close()
        srv.close()

    t = threading.Thread(target=run, daemon=True)
    t.start()
    return t


def test_proxy_with_fake_server():
    if os.name == 'nt':
        return
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        os.environ['JUIZ_IPC_DIR'] = d
        try:
            assert ipc_address('fake') == os.path.join(d, 'fake.sock')
            t = _fake_server(ipc_address('fake'), [
                ('response', {'ok': 1}),
                ('error', {'class_name': 'process', 'function_name': 'call', 'message': 'boom'}),
                ('response', Image.new('L', (2, 2), 5)),
            ])
            p = IpcProxy('fake')
            assert p.read('system', 'profile_full') == {'ok': 1}
            try:
                p.update('process', 'call', {'x': 1}, {'identifier': 'core://core/process/a::b'})
                raise AssertionError('error response must raise')
            except IpcRemoteError as e:
                assert str(e) == 'boom' and e.function_name == 'call'
            img = p.update('process', 'call', {'img': Image.new('RGB', (2, 2))})
            assert img.mode == 'L' and img.tobytes() == bytes([5] * 4)
            t.join(5)
            # サーバーが閉じた後の要求は、繋ぎ直そうとして失敗する
            try:
                p.read('system', 'profile_full')
                raise AssertionError('request after the server closed must fail')
            except IpcError:
                pass
        finally:
            del os.environ['JUIZ_IPC_DIR']


if __name__ == '__main__':
    test_values_and_arrays()
    test_images()
    test_padded_stride()
    test_bad_frames()
    test_numpy()
    test_proxy_with_fake_server()
    print('ipc_frame_test passed')
