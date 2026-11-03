from juiz import juiz_container_process, set_pose
from juiz_std.geometry import Pose, PoseStamped, Point3D, Quaternion


@juiz_container_process(container_type="py_part")
def py_part_set_pose(part, reference: str = "map", x: float = 0.0, y: float = 0.0, z: float = 0.0,
                     qx: float = 0.0, qy: float = 0.0, qz: float = 0.0, qw: float = 1.0) -> PoseStamped:
    """自分を基準 reference から見て (x, y, z)・(qx, qy, qz, qw) にする"""
    p = Pose(position=Point3D(x=x, y=y, z=z), orientation=Quaternion(x=qx, y=qy, z=qz, w=qw))
    return set_pose(part, reference, p)
