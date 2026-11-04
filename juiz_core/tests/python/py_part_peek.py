import time

from juiz import juiz_container_process, pose
from juiz_std.geometry import PoseStamped


@juiz_container_process(container_type="py_part", readonly=True)
def py_part_peek(part, reference: str = "map", sleep_ms: int = 0) -> PoseStamped:
    """readonly の ContainerProcess (#377)。sleep_ms だけ待ってから、基準 reference から見た自分の ContainerPose を返す。
    Container を読み出しロックで呼ぶので、同じ Container の readonly 同士は並行に走る"""
    time.sleep(sleep_ms / 1000.0)
    return pose(part, reference)
