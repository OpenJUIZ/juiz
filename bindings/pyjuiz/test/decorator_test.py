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

# 戻り値の宣言の型は PrimitiveManifest (#506)。古い名前 PrimitiveProfile 等も 0.2 系のあいだ使え、
# 名前ごとに初めの 1 回だけ DeprecationWarning が出る
import warnings
import juiz as _juiz_pkg
import juiz.juiz as _juiz_mod
assert increment_process.manifest()['outputs'] == {'type_name': 'Int', 'default_value': '', 'enum_type': 'Primitive'}
with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter('always')
    assert _juiz_pkg.PrimitiveProfile is PrimitiveManifest
    assert _juiz_pkg.PrimitiveProfile is PrimitiveManifest
    assert _juiz_mod.StructProfile is StructManifest
    assert _juiz_pkg.OutputProfile is OutputManifest
deprecations = [w for w in caught if issubclass(w.category, DeprecationWarning)]
assert len(deprecations) == 3, [str(w.message) for w in deprecations]
assert 'PrimitiveManifest' in str(deprecations[0].message)
try:
    _juiz_pkg.NoSuchName
    raise AssertionError('知らない名前でも AttributeError になりませんでした')
except AttributeError:
    pass
print('deprecated aliases ok')
