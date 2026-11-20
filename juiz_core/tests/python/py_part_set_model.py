import juiz
from juiz import juiz_container_process


@juiz_container_process(container_type="py_part")
def py_part_set_model(part, text: str = "", format: str = "sdf") -> dict:
    """モデルを text (空なら外す) に差し替え、差し替えた後のモデル情報を返す (#389)。
    値の誤りはエラーの種類 (例外のクラス名) を返す"""
    try:
        if not text:
            return {"cleared": juiz.clear_model(part) is None}
        return juiz.set_model(part, text.encode("utf-8"), format, color="#00ff00")
    except juiz.ContainerModelInvalidError as e:
        return {"error": type(e).__name__}
