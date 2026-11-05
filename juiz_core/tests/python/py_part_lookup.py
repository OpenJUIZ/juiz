from juiz import juiz_container_process, lookup
from juiz_std.geometry import PoseStamped


@juiz_container_process(container_type="py_part", readonly=True)
def py_part_lookup(part, target: str = "base0", reference: str = "map") -> PoseStamped:
    """自分以外の Container target を基準 reference から見た ContainerPose (#409、tf の lookupTransform)。
    読み出しだけなので readonly の ContainerProcess で呼べる"""
    return lookup(part, target, reference)
