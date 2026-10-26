# 遅延評価のテスト用 (issue #175)。グレースケール (L) の画像を返す
import PIL.Image
from PIL.Image import Image
from juiz import juiz_process

@juiz_process
def py_lazy_gray() -> Image:
    return PIL.Image.new("L", (4, 3), 200)
