import juiz
from juiz import juiz_container_process


@juiz_container_process(container_type="py_part", readonly=True)
def py_part_try_readonly(part, op: str = "pose", reference: str = "map") -> str:
    """readonly の中から読み書きを試し、エラーの種類 (例外のクラス名) か "ok" を返す (#377)"""
    try:
        if op == "pose":
            juiz.pose(part, reference)
        elif op == "set_pose":
            juiz.set_pose(part, reference, {"position": {"x": 1.0}})
        return "ok"
    except juiz.TransformationsError as e:
        return type(e).__name__
