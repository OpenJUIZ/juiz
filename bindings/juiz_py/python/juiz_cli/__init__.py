"""PyPI パッケージ juiz-cli の `juiz` コマンド。

Python の Process などを作るためのデコレータは pyjuiz(``import juiz``)にある。
このパッケージは pyjuiz に依存しているので、``pip install juiz-cli`` だけで両方使える。
"""

import os
import sys

from ._native import main as _main


def _launcher_path() -> "str | None":
    """この ``juiz`` を起動した実行ファイル(pip のランチャ)の絶対パス。

    Rust 側の ``current_exe()`` は python.exe を指すので、``juiz associate`` や基礎の別窓起動に使う
    juiz の実行ファイルはここで求めて環境変数 ``JUIZ_LAUNCHER`` で渡す(docs/spec/app-package.md §17.2)。
    Windows の pip のランチャは ``sys.argv[0]`` から ``.exe`` を落とすので付け直す。
    ``python -m juiz_cli`` のように実行ファイルでないものから起動したときは None。
    """
    if not sys.argv or not sys.argv[0]:
        return None
    path = os.path.abspath(sys.argv[0])
    if os.name == "nt" and not path.lower().endswith(".exe") and os.path.isfile(path + ".exe"):
        path += ".exe"
    if not os.path.isfile(path) or path.lower().endswith((".py", ".pyc")):
        return None
    return path


def main() -> None:
    """``juiz`` コマンドのエントリポイント。"""
    # 親の juiz から受け継いだ値より、自分のランチャを優先する(別の venv の juiz から起動されたときのため)
    launcher = _launcher_path()
    if launcher:
        os.environ["JUIZ_LAUNCHER"] = launcher
    sys.exit(_main(sys.argv))
