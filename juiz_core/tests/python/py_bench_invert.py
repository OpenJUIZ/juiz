# Composite のベンチマーク用 (issue #169)。色の反転
from PIL import ImageOps
from PIL.Image import Image
from juiz import juiz_process

@juiz_process
def py_bench_invert(img: Image) -> Image:
    return ImageOps.invert(img)
