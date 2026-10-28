# 遅延評価のテスト用 (issue #175)。受け取った引数の型名を返す
from juiz import juiz_process

@juiz_process
def py_lazy_type_name(x: dict = {}) -> str:
    return type(x).__module__ + "." + type(x).__qualname__
