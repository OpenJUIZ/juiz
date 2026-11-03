import juiz
from juiz import juiz_container_process


@juiz_container_process(container_type="py_part")
def py_part_try(part, op: str = "pose", reference: str = "map") -> str:
    """読み書きを試し、エラーの種類 (例外のクラス名) か "ok" を返す"""
    try:
        if op == "pose":
            juiz.pose(part, reference)
        elif op == "set_pose":
            juiz.set_pose(part, reference, {"position": {"x": 1.0}})
        elif op == "set_pose_zero":
            # 長さ 0 の四元数
            juiz.set_pose(part, reference, {"orientation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 0.0}})
        elif op == "set_pose_stamped_dict":
            # PoseStamped の dict も受け付ける (pose だけを使う)
            juiz.set_pose(part, reference, {"header": {"frame_id": "x"}, "pose": {"position": {"x": 2.0}}})
        elif op == "not_container":
            juiz.pose({"a": 1}, reference)
        return "ok"
    except juiz.TransformationsError as e:
        return type(e).__name__
