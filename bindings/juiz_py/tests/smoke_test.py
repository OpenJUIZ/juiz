"""pip で入れた juiz コマンドのスモークテスト(issue #88)。

wheel をインストールした環境で実行する:

    python bindings/juiz_py/tests/smoke_test.py [juiz_examples_python のディレクトリ]

Python の Process を読み込んだ juiz を起動し、HTTP 経由で呼び出して結果を確かめたあと、
Ctrl+C(Windows では CTRL_BREAK_EVENT)で止まることを確かめる。
juiz_examples_python のディレクトリを渡すと、そのサンプル(Process と Container)も同じように動かす。
"""

import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

PORT = 18080

PROCESS = '''\
from juiz import juiz_process


@juiz_process
def increment_process_python(arg1: int = 0) -> int:
    return arg1 + 1
'''

CONF = f'''\
name: juiz_py_smoke_test
option:
  http_broker:
    start: true
    port: {PORT}
plugins:
  process_factories:
    increment_process_python:
      language: python
      path: "./process"
processes:
  - type_name: increment_process_python
    name: increment0
'''


def wait_for_port(port: int, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=1):
                return
        except OSError:
            time.sleep(0.5)
    raise TimeoutError(f"juiz が {timeout} 秒以内にポート {port} で待ち受けを始めませんでした")


def wait_for_port_free(port: int, timeout: float) -> None:
    """前の juiz が使っていたポートを再び待ち受けられるようになるまで待つ。"""
    deadline = time.monotonic() + timeout
    while True:
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.bind(("127.0.0.1", port))
                return
        except OSError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.5)


def run_case(juiz: str, cwd: str, conf: str, port: int, calls: list) -> bool:
    """cwd で juiz -f conf -d を起動し、calls の各コマンドの出力に期待する文字列が含まれるか確かめてから止める。

    calls は (juiz のサブコマンドと引数のリスト, 出力に含まれるべき文字列) のリスト。
    """
    print(f"--- {conf}")
    wait_for_port_free(port, 30)
    kwargs = {}
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    server = subprocess.Popen([juiz, "-f", conf, "-d"], cwd=cwd, **kwargs)
    try:
        wait_for_port(port, 60)
        for args, expected in calls:
            result = subprocess.run(
                [juiz, *args], cwd=cwd, check=True, capture_output=True, text=True,
            )
            print(result.stdout.strip())
            if expected not in result.stdout:
                print(f"juiz {' '.join(args)} の出力に {expected} が含まれていません", file=sys.stderr)
                return False

        server.send_signal(signal.CTRL_BREAK_EVENT if os.name == "nt" else signal.SIGINT)
        try:
            server.wait(timeout=15)
        except subprocess.TimeoutExpired:
            print("Ctrl+C で juiz が止まりませんでした", file=sys.stderr)
            return False
    finally:
        if server.poll() is None:
            server.kill()
            server.wait()
    return True


def run_examples(juiz: str, examples: str) -> bool:
    """juiz_examples_python のサンプルを README の手順どおりに動かす(ポートは 8000)。"""
    base = "http://127.0.0.1:8000"
    if not run_case(juiz, examples, "increment_process.conf", 8000, [
        (["process", "call", f"{base}/process/increment0::increment_process_python", '{"arg1": 3}', "-p"],
         "Value(4)"),
    ]):
        return False
    increment = f"{base}/container_process/increment0::example_container_python_increment:container0"
    get = f"{base}/container_process/get0::example_container_python_get:container0"
    return run_case(juiz, examples, "example_container.conf", 8000, [
        (["container-process", "call", increment, '{"arg1": 5}', "-p"], "Value(5)"),
        (["container-process", "call", get, "{}", "-p"], "Value(5)"),
    ])


def main() -> int:
    juiz = shutil.which("juiz")
    if juiz is None:
        print("juiz コマンドが見つかりません(wheel をインストールしてください)", file=sys.stderr)
        return 1
    subprocess.run([juiz, "--help"], check=True, stdout=subprocess.DEVNULL)

    with tempfile.TemporaryDirectory() as work:
        os.makedirs(os.path.join(work, "process"))
        with open(os.path.join(work, "process", "increment_process_python.py"), "w", encoding="utf-8") as f:
            f.write(PROCESS)
        with open(os.path.join(work, "smoke.conf"), "w", encoding="utf-8") as f:
            f.write(CONF)

        url = f"http://127.0.0.1:{PORT}/process/increment0::increment_process_python"
        if not run_case(juiz, work, "smoke.conf", PORT, [
            (["process", "call", url, '{"arg1": 3}', "-p"], "Value(4)"),
        ]):
            return 1

    if len(sys.argv) > 1 and not run_examples(juiz, sys.argv[1]):
        return 1

    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
