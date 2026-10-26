# 遅延評価のテスト用 (issue #175)。juiz の値に変換できないオブジェクトを返す。
# Python 同士ならそのまま渡るので変換は起きず、Python 以外が読むと変換のエラーになる
from juiz import juiz_process

class Opaque:
    pass

@juiz_process
def py_lazy_opaque() -> dict:
    return Opaque()
