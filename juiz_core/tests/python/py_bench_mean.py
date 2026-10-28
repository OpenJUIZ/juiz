# Composite のベンチマーク用 (issue #169)。画素値の平均
from PIL import ImageStat
from PIL.Image import Image
from juiz import juiz_process

@juiz_process
def py_bench_mean(img: Image) -> float:
    return float(sum(ImageStat.Stat(img).mean) / 3)
