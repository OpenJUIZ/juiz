"""生成した client stub (juiz-idl -g python) で HTTP 越しに Python の Process を呼ぶテスト (issue #194)

juiz_core/tests/integration/python_datatype_test.rs が juiz を起動してから実行する:

    python test/typed_client_test.py <port>
"""
import os
import sys

sys.path.append('.')
sys.path.append(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'gen'))

from juiz.proxy import ProcessProxy, InternalServerError
from robot.geometry import MotorMode, MoveToClient, Point2D, Pose2D

port = int(sys.argv[1])

move_to = MoveToClient.connect('move_to0', port=port)
r = move_to(Pose2D(position=Point2D(x=1.0, y=2.0), theta=0.5), speed=2.0)
assert isinstance(r, Pose2D), r
assert r == Pose2D(position=Point2D(x=2.0, y=4.0), theta=0.5, comment='moved'), r
# 既定値の引数は省ける
assert move_to({'position': {'x': 1, 'y': 1}}).position.x == 1.0

# ふつうの ProcessProxy にも生成した型の値を渡せる (辞書にして送る)
mode = ProcessProxy('py_typed_mode0', 'localhost', port)
assert mode(MotorMode.Idle) == 'Velocity'
assert MotorMode.from_value(mode(mode=MotorMode.Position)) is MotorMode.Idle

# 型に合わない辞書はサーバー側の Python がエラーにする
try:
    ProcessProxy('py_typed_pose_origin0', 'localhost', port)({'position': {'x': 'a', 'y': 0}})
    raise AssertionError('no error')
except InternalServerError as e:
    assert 'position.x' in str(e), str(e)

print('typed_client_test: OK')
