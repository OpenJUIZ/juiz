# データ型 Step 5 (issue #194) のテスト用。受け取った Pose2D がどこから来たかを返す
# (Python のネイティブ値ならその目印、辞書から作られたものなら "dict:<x>")
from juiz import juiz_process
from robot.geometry import Pose2D


@juiz_process
def py_typed_pose_origin(pose: Pose2D) -> str:
    assert isinstance(pose, Pose2D), type(pose)
    return getattr(pose, '_made_by', 'dict:%s' % pose.position.x)
