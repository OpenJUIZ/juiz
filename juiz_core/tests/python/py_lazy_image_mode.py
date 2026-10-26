# 遅延評価のテスト用 (issue #175)。受け取った画像のモードを返す
from PIL.Image import Image
from juiz import juiz_process

@juiz_process
def py_lazy_image_mode(img: Image) -> str:
    return img.mode
