from juiz import juiz_container_process, pose
from juiz_std.geometry import PoseStamped


@juiz_container_process(container_type="py_part")
def py_part_pose(part, reference: str = "map") -> PoseStamped:
    """基準 reference から見た自分の ContainerPose"""
    p = pose(part, reference)
    assert isinstance(p, PoseStamped)
    return p
