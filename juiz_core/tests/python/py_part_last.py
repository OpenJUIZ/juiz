from juiz import juiz_container_process


@juiz_container_process(container_type="py_part")
def py_part_last(part) -> dict:
    """Pose 通知で最後に受け取った値 (なければ空の dict)"""
    return part.last.to_value() if part.last is not None else {}
