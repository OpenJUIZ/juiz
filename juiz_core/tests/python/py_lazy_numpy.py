# 遅延評価のテスト用 (issue #175)。numpy の配列を返す (Python 同士ならそのまま渡る)
import numpy
from juiz import juiz_process

@juiz_process
def py_lazy_numpy(n: int = 3) -> list:
    return numpy.arange(n, dtype=float)
