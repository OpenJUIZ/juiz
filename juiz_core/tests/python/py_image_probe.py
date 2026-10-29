# Python への画像の渡し方のテスト用 (issue #270)。モードと大きさと左上・右下の画素を文字列で返す
from PIL.Image import Image
from juiz import juiz_process

@juiz_process
def py_image_probe(img: Image) -> str:
    w, h = img.size
    return f"{img.mode} {w}x{h} {img.getpixel((0, 0))} {img.getpixel((w - 1, h - 1))}"
