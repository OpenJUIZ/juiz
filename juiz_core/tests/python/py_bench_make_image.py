# Composite のベンチマーク用 (issue #169)。seed で色を変えた 640x480 の RGB 画像を作る
import PIL.Image
from PIL.Image import Image
from juiz import juiz_process

@juiz_process
def py_bench_make_image(seed: int = 0) -> Image:
    return PIL.Image.new("RGB", (640, 480), (seed % 256, 64, 128))
