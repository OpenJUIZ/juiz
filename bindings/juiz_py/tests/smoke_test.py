"""pip で入れた juiz コマンドのスモークテスト(issue #88)。

wheel をインストールした環境で実行する:

    python bindings/juiz_py/tests/smoke_test.py

Python の Process を読み込んだ juiz を起動し、HTTP 経由で呼び出して結果を確かめたあと、
Ctrl+C(Windows では CTRL_BREAK_EVENT)で止まることを確かめる。
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

        kwargs = {}
        if os.name == "nt":
            kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
        server = subprocess.Popen([juiz, "-f", "smoke.conf", "-d"], cwd=work, **kwargs)
        try:
            wait_for_port(PORT, 60)
            url = f"http://127.0.0.1:{PORT}/process/increment0::increment_process_python"
            result = subprocess.run(
                [juiz, "process", "call", url, '{"arg1": 3}', "-p"],
                cwd=work, check=True, capture_output=True, text=True,
            )
            print(result.stdout.strip())
            if "Value(4)" not in result.stdout:
                print("Process の呼び出し結果が 4 になりませんでした", file=sys.stderr)
                return 1

            server.send_signal(signal.CTRL_BREAK_EVENT if os.name == "nt" else signal.SIGINT)
            try:
                server.wait(timeout=15)
            except subprocess.TimeoutExpired:
                print("Ctrl+C で juiz が止まりませんでした", file=sys.stderr)
                return 1
        finally:
            if server.poll() is None:
                server.kill()
                server.wait()

    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
