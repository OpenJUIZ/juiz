"""PyPI パッケージ juiz の `juiz` コマンド。

Python の Process などを作るためのデコレータは pyjuiz(``import juiz``)にある。
このパッケージは pyjuiz に依存しているので、``pip install juiz`` だけで両方使える。
"""

import sys

from ._native import main as _main


def main() -> None:
    """``juiz`` コマンドのエントリポイント。"""
    sys.exit(_main(sys.argv))
