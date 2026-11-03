from juiz import juiz_container


class PyPart:
    """Transformations のテスト (#371) の部品。Pose 通知で受け取った値も覚える"""
    def __init__(self):
        self.last = None


@juiz_container
def py_part():
    return PyPart()
