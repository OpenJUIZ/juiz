# データ型 Step 5 (issue #194) のテスト用。既定値つきの列挙の引数を受け取り、次の記号を返す
from juiz import juiz_process
from robot.geometry import MotorMode


@juiz_process
def py_typed_mode(mode: MotorMode = MotorMode.Velocity) -> MotorMode:
    symbols = list(MotorMode)
    return symbols[(symbols.index(mode) + 1) % len(symbols)]
