# Composite のベンチマーク用 (issue #169)。左右反転
import PIL.Image
from PIL.Image import Image
from juiz import juiz_process

@juiz_process
def py_bench_flip(img: Image) -> Image:
    return img.transpose(PIL.Image.FLIP_LEFT_RIGHT)
