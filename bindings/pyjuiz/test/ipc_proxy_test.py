"""IPC ブローカーを立てた juiz に pyjuiz の IPC クライアントで繋ぐテスト (issue #236)

juiz_core/tests/integration/pyjuiz_ipc_proxy_test.rs が System を起動してから、このスクリプトを
    python test/ipc_proxy_test.py <IPC ブローカーの名前> <HTTP のポート>
で実行する。Process と Container は ipc_broker_api_test.rs の server_manifest と setup_test_processes のもの
(inc0 / inc1、c0 の c0inc / c0get、画像を作る make0、画像を返す echo0、値を返す vecho0、必ず失敗する fail0)。
同じ Process を HTTP でも呼び、HTTP の画像 (PNG) と位置で渡す複数の引数が今までどおり動くことと、速さを比べる。
"""
import sys
import time

import numpy as np
from PIL import Image

from juiz.proxy import (ProcessProxy, ContainerProxy, SystemProxy, IpcProxy, IpcError, IpcRemoteError,
                        InternalServerError, NotFoundError, connect)


def check_process(name):
    inc = ProcessProxy("inc0", ipc=name)
    assert inc.identifier().startswith(f"ipc://{name}/process/inc0::"), inc.identifier()
    assert inc.call(10) == 11
    assert inc(20) == 21
    inc.p_apply("arg1", 1)
    assert inc.execute() == 2
    # protocol='ipc' なら host が IPC ブローカーの名前
    assert ProcessProxy("inc0", name, protocol="ipc").call(1) == 2
    try:
        ProcessProxy("nothing0", ipc=name)
        raise AssertionError("ProcessProxy for a missing process must fail")
    except NotFoundError as e:
        assert f"ipc://{name}" in str(e), str(e)


def check_container(name):
    c0 = ContainerProxy("c0", ipc=name)
    before = c0.c0get.call()
    assert c0.c0inc.call(3) == before + 3
    assert c0.c0get() == before + 3


def check_system(name):
    system = connect(f"ipc://{name}")
    assert isinstance(system.crud, IpcProxy)
    profile = system.profile_full()
    assert isinstance(profile, dict) and profile, profile
    names = sorted(p.name() for p in system.processes())
    for n in ["inc0", "inc1", "make0", "echo0", "vecho0", "fail0"]:
        assert n in names, names
    assert [c.name() for c in system.containers()] == ["c0"]


def rgb8_pixel(x, y):
    # ipc_broker_api_test.rs の image_of_format("rgb8") と同じ
    return (x % 256, y % 256, (x ^ y) % 256)


def check_images(name):
    make = ProcessProxy("make0", ipc=name)
    echo = ProcessProxy("echo0", ipc=name)

    # 戻り値の画像。PIL で表せる形式は PIL の Image
    img = make.call(format="rgb8", width=1280, height=720)
    assert isinstance(img, Image.Image) and img.mode == "RGB" and img.size == (1280, 720), img
    for x, y in [(0, 0), (1279, 719), (300, 500)]:
        assert img.getpixel((x, y)) == rgb8_pixel(x, y), (x, y, img.getpixel((x, y)))
    # 引数が 2 つ以上でも位置で渡せる (以前は HTTP で文字列になっていた)
    small = make.call("l8", 5, 3)
    assert small.mode == "L" and small.size == (5, 3), small
    assert small.getpixel((4, 2)) == 4 * 3 + 2 * 7

    for fmt, mode in [("l8", "L"), ("la8", "LA"), ("rgba8", "RGBA"), ("l16", "I;16")]:
        got = make.call(format=fmt, width=7, height=4)
        assert isinstance(got, Image.Image) and got.mode == mode and got.size == (7, 4), (fmt, got)
    l16 = make.call(format="l16", width=7, height=4)
    assert l16.getpixel((6, 3)) == 6 * 300 + 3, l16.getpixel((6, 3))

    # PIL で表せない形式は numpy の配列 (高さ × 幅 × チャンネル)
    rgb16 = make.call(format="rgb16", width=7, height=4)
    assert isinstance(rgb16, np.ndarray) and rgb16.dtype == np.uint16 and rgb16.shape == (4, 7, 3), rgb16
    assert list(rgb16[3, 6]) == [600, 600, 65535], rgb16[3, 6]
    f32 = make.call(format="rgba32f", width=7, height=4)
    assert f32.dtype == np.float32 and f32.shape == (4, 7, 4), f32
    assert np.allclose(f32[3, 6], [6 / 7.0, -1.5, 1e-3, 1.0]), f32[3, 6]

    # 引数の画像。送った画素がそのまま返る
    for mode in ["L", "LA", "RGB", "RGBA", "I;16"]:
        src = img.convert("RGBA" if mode in ("LA", "RGBA") else "RGB").crop((0, 0, 64, 48)).convert(mode) \
            if mode != "I;16" else Image.frombytes("I;16", (64, 48), bytes(range(256)) * 24)
        back = echo.call(src)
        assert back.mode == src.mode and back.tobytes() == src.tobytes(), mode
    # パレットの画像は RGB にして送る
    pal = img.crop((0, 0, 16, 16)).convert("P")
    assert echo.call(pal).tobytes() == pal.convert("RGB").tobytes()
    # numpy の配列も画像として送れる
    for arr in [np.arange(48 * 64 * 3, dtype=np.uint16).reshape(48, 64, 3),
                np.linspace(-1, 1, 48 * 64 * 4, dtype=np.float32).reshape(48, 64, 4),
                np.arange(48 * 64, dtype=np.uint8).reshape(48, 64)]:
        back = echo.call(arr)
        back = back if isinstance(back, np.ndarray) else np.asarray(back)
        assert back.shape == arr.shape and np.array_equal(back, arr), (arr.dtype, arr.shape)
    # 行の途中で切れた (連続でない) 配列も送れる
    view = np.arange(48 * 128 * 3, dtype=np.uint8).reshape(48, 128, 3)[:, ::2, :]
    assert np.array_equal(np.asarray(echo.call(view)), view)

    # 720p の往復の時間 (目安として出すだけ)
    start = time.perf_counter()
    n = 20
    for _ in range(n):
        back = echo.call(img)
    elapsed = (time.perf_counter() - start) / n * 1000
    assert back.tobytes() == img.tobytes()
    print(f"720p rgb8 echo over ipc: {elapsed:.2f} ms")


def check_values(name):
    vecho = ProcessProxy("vecho0", ipc=name)
    ints = list(range(-1000, 2000))
    got = vecho.call(ints)
    assert got == ints and all(type(e) is int for e in got)
    floats = [i * 0.5 for i in range(3000)]
    got = vecho.call(floats)
    assert got == floats and all(type(e) is float for e in got)
    # 整数と小数が混ざった配列は JSON のまま送り、区別が保たれる
    mixed = [1, 2.0] * 600
    got = vecho.call(mixed)
    assert got == mixed and [type(e) for e in got[:2]] == [int, float], got[:2]
    # 真偽値の配列は数の配列にしない
    bools = [True, False] * 600
    assert vecho.call(bools) == bools
    nested = {"a": [1, 2.5, "x", None], "b": {"c": True}, "s": "日本語"}
    assert vecho.call(nested) == nested


def check_errors(name):
    fail = ProcessProxy("fail0", ipc=name)
    try:
        fail.call()
        raise AssertionError("fail0 must fail")
    except IpcRemoteError as e:
        assert "fails on purpose" in str(e), str(e)
        assert isinstance(e, InternalServerError)
    # エラーの後も同じ接続で呼べる
    assert fail.system.crud.read("system", "uuid")

    try:
        IpcProxy("../bad")
        raise AssertionError("invalid ipc name must fail")
    except ValueError:
        pass
    try:
        SystemProxy(IpcProxy("no_such_juiz_server")).profile_full()
        raise AssertionError("connecting to a missing server must fail")
    except IpcError as e:
        assert "no_such_juiz_server" in str(e), str(e)


def check_reconnect(name):
    proxy = IpcProxy(name)
    system = SystemProxy(proxy)
    assert system.profile_full()
    # 接続を閉じても次の要求で繋ぎ直す
    proxy.close()
    assert system.profile_full()
    with IpcProxy(name) as p:
        assert p.read("system", "uuid")


def check_http(port):
    make = ProcessProxy("make0", "localhost", port)
    echo = ProcessProxy("echo0", "localhost", port)
    # 位置で渡す複数の引数は JSON の値のまま届く (以前は文字列になっていた)
    small = make.call("l8", 5, 3)
    assert small.size == (5, 3) and small.getpixel((4, 2))[0] == 4 * 3 + 2 * 7, small
    img = make.call(format="rgb8", width=1280, height=720)
    assert img.size == (1280, 720) and img.getpixel((300, 500)) == rgb8_pixel(300, 500)
    start = time.perf_counter()
    n = 5
    for _ in range(n):
        back = echo.call(img)
    elapsed = (time.perf_counter() - start) / n * 1000
    assert back.tobytes() == img.tobytes()
    print(f"720p rgb8 echo over http: {elapsed:.2f} ms")
    vecho = ProcessProxy("vecho0", "localhost", port)
    assert vecho.call({"a": [1, 2.5]}) == {"a": [1, 2.5]}


if __name__ == '__main__':
    name = sys.argv[1]
    port = int(sys.argv[2])
    check_process(name)
    check_container(name)
    check_system(name)
    check_images(name)
    check_values(name)
    check_errors(name)
    check_reconnect(name)
    check_http(port)
    print("ipc_proxy_test passed")
