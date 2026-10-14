from juiz import juiz_process

@juiz_process
def increment_process(arg1:int) -> int:
    print('pyadd ', arg1, '>', arg1+1)
    return arg1 + 1
