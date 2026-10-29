# Python への画像の渡し方のベンチ用 (issue #270)。画像の幅だけを返す (変換以外の時間を小さくする)
from PIL.Image import Image
from juiz import juiz_process

@juiz_process
def py_image_width(img: Image) -> int:
    return img.width
