# データ型 Step 5 (issue #194) のテスト用。geometry.jidl の process move_to を Python で書いたもの
# (bindings/pyjuiz/test/typed_client_test.py が生成した MoveToClient で呼ぶ)
from juiz import juiz_process
from robot.geometry import Point2D, Pose2D


@juiz_process
def move_to(target: Pose2D, speed: float = 1.0) -> Pose2D:
    """指定の姿勢まで動く"""
    return Pose2D(position=Point2D(x=target.position.x * speed, y=target.position.y * speed),
                  theta=target.theta, frame=target.frame, comment='moved')
