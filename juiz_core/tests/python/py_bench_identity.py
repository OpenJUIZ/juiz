# Composite のベンチマーク用 (issue #169)。何もしない。段の受け渡しにかかる時間を測る
from PIL.Image import Image
from juiz import juiz_process

@juiz_process
def py_bench_identity(img: Image) -> Image:
    return img
