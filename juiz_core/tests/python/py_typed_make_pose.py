# データ型 Step 5 (issue #194) のテスト用。juiz-idl が生成した型 (bindings/pyjuiz/test/gen) を返す
from juiz import juiz_process
from robot.geometry import Point2D, Pose2D


@juiz_process
def py_typed_make_pose(x: float = 0.0) -> Pose2D:
    pose = Pose2D(position=Point2D(x=x, y=0.0))
    # Python 同士ならこのオブジェクトがそのまま渡る (辞書を通ると消える) ことの目印
    pose._made_by = 'py_typed_make_pose'
    return pose
