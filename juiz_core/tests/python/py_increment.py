from juiz import juiz_process

@juiz_process
def py_increment(arg1: int = 1) -> int:
    return arg1 + 1
