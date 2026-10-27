"""juiz-idl の Python 出力と pyjuiz のデータ型対応 (issue #194) のテスト

test/gen は juiz_idl/test_package/idl/geometry.jidl から juiz-idl -g python で作ったもの
(juiz_idl の python_gen_is_up_to_date テストが最新かを確かめる)。bindings/pyjuiz で実行する:

    python test/datatype_test.py
"""
import os
import sys

sys.path.append('.')
sys.path.append(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'gen'))

import dataclasses

from juiz import juiz_process, juiz_container, juiz_container_process, datatype, ValueDoesNotMatchTypeError
from robot.common import Header, Stamp
from robot.geometry import Everything, MotorMode, MoveToClient, MotorClient, Point2D, Pose2D


def expect_error(f, *texts):
    try:
        f()
    except ValueDoesNotMatchTypeError as e:
        for t in texts:
            assert t in str(e), str(e)
        return e
    raise AssertionError('ValueDoesNotMatchTypeError was not raised')


# --- 生成した型と辞書の変換 (仕様 §4) ---

pose = Pose2D(position=Point2D(x=1.0, y=2.0))
assert pose.to_value() == {'position': {'x': 1.0, 'y': 2.0}, 'theta': 0.0, 'frame': 'map', 'comment': None}
# 足りないフィールドは既定値、型にないキーは無視
assert Pose2D.from_value({'position': {'x': 1, 'y': 2}, 'unknown': 1}) == pose
assert isinstance(Pose2D.from_value({'position': {'x': 1, 'y': 2}}).position.x, float)
expect_error(lambda: Pose2D.from_value({}), 'robot.geometry.Pose2D.position', 'no default')
expect_error(lambda: Pose2D.from_value({'position': {'x': 'a', 'y': 0}}), 'Pose2D.position.x', 'expected a number')
expect_error(lambda: Pose2D.from_value([1]), 'expected an object')

e = Everything(header=Header(stamp=Stamp()))
assert e.data == b'\x00\x01\x02' and e.list == [1, 2] and e.extra == {'free': [1, 'x']}
v = e.to_value()
assert v['data'] == 'AAEC'  # bytes は base64
assert v['header'] == {'frame_id': 'map', 'stamp': {'sec': 0, 'nanosec': 0}}
assert v['type'] == 't' and v['self'] == 0
assert Everything.from_value(v) == e
# 既定値の list は別々のオブジェクト
assert Everything(header=Header(stamp=Stamp())).list is not e.list
expect_error(lambda: Everything.from_value({'header': {'stamp': {}}, 'i': 2 ** 31}), 'Everything.i', 'out of the range of int')
expect_error(lambda: Everything.from_value({'header': {'stamp': {}}, 'i': 1.0}), 'expected an integer')
expect_error(lambda: Everything.from_value({'header': {'stamp': {}}, 'flag': 1}), 'expected a boolean')
expect_error(lambda: Everything.from_value({'header': {'stamp': {}}, 'data': '***'}), 'base64')
expect_error(lambda: Everything(header=Header(stamp=Stamp()), f=float('nan')).to_value(), 'NaN')

# 列挙: 知らない記号は既定値
assert MotorMode.from_value('Position') is MotorMode.Position
assert MotorMode.from_value('Unknown') is MotorMode.Idle
assert MotorMode.Velocity.to_value() == 'Velocity'
assert Everything.from_value({'header': {'stamp': {}}, 'mode': 'Velocity'}).mode is MotorMode.Velocity

# 型参照
schema = Pose2D.type_schema()
assert schema['name'] == 'robot.geometry.Pose2D'
assert schema['fingerprint'].startswith('sha256:') and len(schema['fingerprint']) == 71
assert schema['avro']['type'] == 'record'
assert datatype.is_juiz_type(Pose2D) and datatype.is_juiz_type(MotorMode)
assert not datatype.is_juiz_type(dict) and not datatype.is_juiz_type(pose)
# 写しを返す
schema['name'] = 'x'
assert Pose2D.type_schema()['name'] == 'robot.geometry.Pose2D'


# --- デコレータ: 生成した型の引数・戻り値 ---

@juiz_process
def move(pose: Pose2D, dx: float = 1.0) -> Pose2D:
    pose.position.x += dx
    return pose

m = move.manifest()
args = {a['name']: a for a in m['arguments']}
assert args['pose']['type_name'] == 'Object'
assert args['pose']['schema'] == Pose2D.type_schema()
assert 'default' not in args['pose']  # 既定値なし
assert 'schema' not in args['dx']  # 基本型は今のとおり型なし
assert m['output_schema'] == Pose2D.type_schema()
assert m['outputs'] == {'type_name': 'Object', 'default_value': '', 'enum_type': 'Primitive'}

# 同じ型の値はそのまま渡り、辞書は型の値にして渡る
p = Pose2D(position=Point2D(x=0.0, y=0.0))
assert move(p) is p and p.position.x == 1.0
r = move({'position': {'x': 1.0, 'y': 0.0}}, 2.0)
assert isinstance(r, Pose2D) and r.position.x == 3.0
r = move(pose={'position': {'x': 1.0, 'y': 0.0}})
assert r.position.x == 2.0
expect_error(lambda: move({'position': {}}), "position.x")


@juiz_process
def next_mode(mode: MotorMode = MotorMode.Velocity) -> MotorMode:
    return {MotorMode.Idle: 'Velocity', MotorMode.Velocity: MotorMode.Position}.get(mode, MotorMode.Idle)

m = next_mode.manifest()
assert m['arguments'][0]['type_name'] == 'String'
assert m['arguments'][0]['default'] == 'Velocity'  # 既定値は辞書の表し方
assert m['outputs']['type_name'] == 'String'
assert next_mode('Velocity') is MotorMode.Position
# 戻り値が辞書の表し方でも型の値にする
assert next_mode(MotorMode.Idle) is MotorMode.Velocity


# from __future__ import annotations で文字列になった型注釈も型にする
exec('''
from __future__ import annotations
@juiz_process
def stringified(pose: Pose2D) -> Header:
    return {"stamp": {"sec": 1}}
''', globals())
m = stringified.manifest()
assert m['arguments'][0]['schema']['name'] == 'robot.geometry.Pose2D'
assert m['output_schema']['name'] == 'robot.common.Header'
assert stringified({'position': {'x': 0, 'y': 0}}) == Header(stamp=Stamp(sec=1))


# 生成していない dataclass は今のとおり型なし (Object)
@dataclasses.dataclass
class Plain:
    a: int = 1

@juiz_process
def plain(p: Plain) -> Plain:
    return Plain(a=p['a'] + 1)

m = plain.manifest()
assert 'schema' not in m['arguments'][0] and 'output_schema' not in m
assert plain({'a': 1}) == {'a': 2}


# Container と ContainerProcess
@juiz_container
def holder(initial: Pose2D) -> dict:
    return {'pose': initial}

m = holder.manifest()
assert m['arguments'][0]['schema']['name'] == 'robot.geometry.Pose2D'
assert holder({'position': {'x': 5, 'y': 0}})['pose'].position.x == 5.0

@juiz_container_process(container_type='holder')
def holder_set(container, mode: MotorMode) -> Header:
    container['mode'] = mode
    return Header(stamp=Stamp(sec=2))

m = holder_set.manifest()
assert [a['name'] for a in m['arguments']] == ['mode']
assert m['output_schema']['name'] == 'robot.common.Header'
c = {}
assert holder_set(c, 'Position').stamp.sec == 2 and c['mode'] is MotorMode.Position


# --- client stub (juiz-idl が process / container 宣言から作る) ---

class FakeProxy:
    """ProcessProxy の代わり。call(body=...) に渡された辞書を覚え、決めた値を返す"""

    def __init__(self, result):
        self.result = result
        self.body = None

    def call(self, *args, **kwargs):
        self.body = kwargs['body']
        return self.result


proxy = FakeProxy({'position': {'x': 9.0, 'y': 1.0}, 'theta': 0.5})
client = MoveToClient(proxy)
r = client(Pose2D(position=Point2D(x=1.0, y=2.0)))
assert r == Pose2D(position=Point2D(x=9.0, y=1.0), theta=0.5)
assert proxy.body == {'target': {'position': {'x': 1.0, 'y': 2.0}, 'theta': 0.0, 'frame': 'map', 'comment': None}, 'speed': 1.0}
# 辞書を渡してもよい (型で検査してから送る)
client({'position': {'x': 0, 'y': 0}}, speed=2)
assert proxy.body['speed'] == 2.0
expect_error(lambda: client({'position': {}}), 'target')
# 戻り値が型に合わなければエラー
expect_error(lambda: MoveToClient(FakeProxy({'theta': 1}))(pose), 'position')


class FakeContainerProxy:
    def __init__(self):
        self.set_mode = FakeProxy(True)
        self.stop = FakeProxy(None)

cp = FakeContainerProxy()
motor = MotorClient(cp)
assert motor.set_mode(MotorMode.Velocity) is True
assert cp.set_mode.body == {'mode': 'Velocity'}
assert motor.stop() is None and cp.stop.body == {}

# proxy (juiz.proxy) は生成した型の値を辞書にして送る
assert datatype.to_value({'a': [MotorMode.Idle, pose]}) == {'a': ['Idle', pose.to_value()]}

print('datatype_test: OK')
