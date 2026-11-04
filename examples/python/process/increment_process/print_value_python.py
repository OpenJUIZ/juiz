from juiz import juiz_process

@juiz_process
def print_value_python(arg1: int = 1) -> int:
    print('print', arg1)
    return arg1
