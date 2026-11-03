from juiz import juiz_container_process
from juiz_std.geometry import PoseStamped


@juiz_container_process(container_type="py_part")
def py_part_on_pose(part, pose: PoseStamped) -> str:
    """Pose 通知の受け手。受け取った値を覚える"""
    assert isinstance(pose, PoseStamped)
    part.last = pose
    return pose.header.frame_id
