"""pyjuiz に同梱する標準の型 juiz_std (issue #199) のテスト

juiz_std は juiz-idl に組み込んだ JIDL から作ったもの (juiz_idl の pyjuiz_std_types_are_up_to_date テストが
最新かを確かめる)。bindings/pyjuiz で実行する:

    python test/std_types_test.py
"""
import sys

sys.path.append('.')

from juiz import juiz_process
from juiz_std.common import Header, Time
from juiz_std.geometry import Point2D, Pose, Pose2D, Quaternion, Twist2D, Vector2
from juiz_std.sensor import ImageInfo, LaserScan

# すべてのフィールドに既定値がある
assert Pose().orientation == Quaternion(x=0.0, y=0.0, z=0.0, w=1.0)
assert ImageInfo().encoding == 'rgb8'
assert ImageInfo().header == Header(stamp=Time(sec=0, nanosec=0), frame_id='')

pose = Pose2D(position=Point2D(x=1.0, y=-2.0), theta=0.5)
assert pose.to_value() == {'position': {'x': 1.0, 'y': -2.0}, 'theta': 0.5}
assert Pose2D.from_value(pose.to_value()) == pose
assert LaserScan.from_value({'ranges': [1.0, 2.5]}).ranges == [1.0, 2.5]

# 指紋は Rust の juiz_std_types と同じ (juiz_idl の std_type_fingerprints_are_fixed で固定)
assert Pose2D.type_schema()['fingerprint'] == 'sha256:548bb61771115ffb247d9e2fcb91b294ca88d6794f3eda9091c3301093d3822b'


# Process の引数・戻り値にそのまま使える
@juiz_process
def drive(pose: Pose2D, twist: Twist2D, dt: float = 1.0) -> Pose2D:
    return Pose2D(position=Point2D(x=pose.position.x + twist.linear.x * dt,
                                   y=pose.position.y + twist.linear.y * dt),
                  theta=pose.theta + twist.angular * dt)

manifest = drive.manifest()
assert manifest['arguments'][0]['schema']['name'] == 'juiz_std.geometry.Pose2D'
assert manifest['output_schema']['name'] == 'juiz_std.geometry.Pose2D'
r = drive(Pose2D(), Twist2D(linear=Vector2(x=1.0, y=0.0), angular=0.1), dt=2.0)
assert r == Pose2D(position=Point2D(x=2.0, y=0.0), theta=0.2), r

print('std_types_test: OK')
