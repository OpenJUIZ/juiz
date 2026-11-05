import juiz
from juiz import juiz_container_process


@juiz_container_process(container_type="py_part")
def py_part_set_transform(part, source: str = "map", destination: str = "base0",
                          x: float = 0.0, y: float = 0.0, z: float = 0.0,
                          qx: float = 0.0, qy: float = 0.0, qz: float = 0.0, qw: float = 1.0) -> dict:
    """Transform (source → destination) の値を書き、書いた後の TransformStamped の辞書を返す (#410)。
    書けない・無いときはエラーの種類 (例外のクラス名) を返す"""
    value = {"translation": {"x": x, "y": y, "z": z}, "rotation": {"x": qx, "y": qy, "z": qz, "w": qw}}
    try:
        return juiz.set_transform(part, source, destination, value).to_value()
    except (juiz.TransformNotWritableError, juiz.TransformNotFoundError) as e:
        return type(e).__name__
