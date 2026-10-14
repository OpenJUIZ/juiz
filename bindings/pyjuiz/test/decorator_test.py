import sys
sys.path.append('.')
sys.path.append('..')

import inspect

from juiz import *

@juiz_process
def increment_process(arg1:int, arg2:float=3.0) -> int:
    """_summary_

    Args:
        arg1 (int, optional): _description_. Defaults to 3.
        arg2 (float, optional): _description_. Defaults to 3.0.

    Returns:
        _type_: _description_
    """
    print('pyadd ', arg1, '>', arg1+1)
    return arg1 + 1


v = increment_process(3)
print('v=', v)

print('manifest = ', increment_process.manifest())

# 型注釈のない引数・戻り値は定義した時点でエラーになる
try:
    @juiz_process
    def no_arg_annotation(arg1:int, arg2=3.0) -> int:
        return arg1
    raise AssertionError('引数の型注釈がなくてもエラーになりませんでした')
except PyJuizMissingTypeAnnotationError as e:
    print('expected error:', e)
    assert e.arg_name == 'arg2'

try:
    @juiz_process
    def no_return_annotation(arg1:int):
        return arg1
    raise AssertionError('戻り値の型注釈がなくてもエラーになりませんでした')
except PyJuizMissingTypeAnnotationError as e:
    print('expected error:', e)
    assert e.arg_name is None
