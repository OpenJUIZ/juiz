# Python と juiz の値の変換を確かめる (issue #221)。kind で決めた型の値を返す
from juiz import juiz_process


@juiz_process
def py_types(kind: str = "bool", value: list = []) -> list:
    if kind == "bool":
        return True
    if kind == "float":
        return 1.5
    if kind == "none":
        return None
    if kind == "tuple":
        return (1, "a")
    if kind == "set":
        return {3}
    if kind == "nested":
        return [[1, 2], {"a": [True, None]}]
    # echo: 受け取った値と、その要素の Python の型名を返す
    return [value, [type(v).__name__ for v in value]]
