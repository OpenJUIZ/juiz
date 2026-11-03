"""juiz.pose / juiz.set_pose (issue #371) の Python 側のテスト

ホストの口 (属性 __juiz_pose__) の代わりに偽物を付け、値の変換とエラーの振り分けを確かめる。
ホストとつないだ確認は juiz_core の tests/integration/python_transformations_test.rs。
bindings/pyjuiz で実行する:

    python test/transformations_test.py
"""
import sys

sys.path.append('.')

import juiz
from juiz_std.geometry import Point3D, Pose, PoseStamped, Quaternion

STAMPED = {
    'header': {'stamp': {'sec': 1, 'nanosec': 2}, 'frame_id': 'map'},
    'pose': {'position': {'x': 1.0, 'y': 0.1, 'z': 0.3}, 'orientation': {'x': 0.0, 'y': 0.0, 'z': 0.5, 'w': 0.5}},
}


class FakeHandle:
    """ホストの ContainerPoseHandle の代わり。エラーはホストと同じ RuntimeError(種類, メッセージ)"""

    def __init__(self, error=None):
        self.error = error
        self.written = None

    def pose(self, reference):
        if self.error:
            raise RuntimeError(self.error, 'message')
        assert isinstance(reference, str)
        return dict(STAMPED, header={'stamp': {'sec': 1, 'nanosec': 2}, 'frame_id': reference})

    def set_pose(self, reference, value):
        if self.error:
            raise RuntimeError(self.error, 'message')
        assert isinstance(value, dict)
        self.written = (reference, value)
        return {'header': {'frame_id': reference}, 'pose': value}


class Part:
    pass


def part(error=None):
    p = Part()
    p.__juiz_pose__ = FakeHandle(error)
    return p


# 読む: PoseStamped の型の値になる
p = juiz.pose(part(), 'map')
assert isinstance(p, PoseStamped)
assert p.header.frame_id == 'map'
assert p.pose.position == Point3D(x=1.0, y=0.1, z=0.3)
assert p.pose.orientation.w == 0.5


# 基準は identifier() を持つもの (ContainerProxy など) も渡せる
class Ref:
    def identifier(self):
        return 'core://core/container/map::static_anchor'


assert juiz.pose(part(), Ref()).header.frame_id == 'core://core/container/map::static_anchor'

# 書く: Pose・PoseStamped・それぞれの dict を受け付け、ホストには Pose の dict が渡る
pose = Pose(position=Point3D(x=2.0), orientation=Quaternion(z=1.0, w=0.0))
for value in [pose, pose.to_value(), PoseStamped(pose=pose), PoseStamped(pose=pose).to_value(), {'position': {'x': 2.0}, 'orientation': {'z': 1.0, 'w': 0.0}}]:
    c = part()
    r = juiz.set_pose(c, 'base0', value)
    assert isinstance(r, PoseStamped)
    reference, written = c.__juiz_pose__.written
    assert reference == 'base0'
    assert Pose.from_value(written) == pose, written
    assert r.pose == pose

# 型の違う値
for bad in [1.0, [1, 2, 3], 'pose']:
    try:
        juiz.set_pose(part(), 'map', bad)
        raise AssertionError('should fail')
    except TypeError:
        pass
try:
    juiz.pose(part(), 1)
    raise AssertionError('should fail')
except TypeError:
    pass

# ホストのエラーは種類ごとの例外になる
for kind, cls in [('NoCommonAnchorError', juiz.NoCommonAnchorError),
                  ('PoseNotWritableError', juiz.PoseNotWritableError),
                  ('InvalidPoseValueError', juiz.InvalidPoseValueError),
                  ('TransformationsNotAttachedError', juiz.TransformationsNotAttachedError),
                  ('ObjectLockError', juiz.TransformationsError)]:
    for f in [lambda c: juiz.pose(c, 'map'), lambda c: juiz.set_pose(c, 'map', pose)]:
        try:
            f(part(kind))
            raise AssertionError('should fail')
        except juiz.TransformationsError as e:
            assert type(e) is cls, (kind, type(e))
            assert e.kind == kind
            assert isinstance(e, RuntimeError)

# juiz が作った Container のオブジェクトでないもの (口がない)
for obj in [{'a': 1}, Part(), None]:
    try:
        juiz.pose(obj, 'map')
        raise AssertionError('should fail')
    except juiz.TransformationsNotAttachedError:
        pass

# 種類のない RuntimeError はそのまま
class RaiseHandle:
    def pose(self, reference):
        raise RuntimeError('plain')


b = Part()
b.__juiz_pose__ = RaiseHandle()
try:
    juiz.pose(b, 'map')
    raise AssertionError('should fail')
except juiz.TransformationsError:
    raise AssertionError('plain RuntimeError must not be converted')
except RuntimeError as e:
    assert e.args == ('plain',)

print('transformations_test: ok')
