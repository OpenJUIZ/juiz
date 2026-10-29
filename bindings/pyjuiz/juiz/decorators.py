import datetime, dataclasses, traceback, io
import inspect
import functools

from .juiz import *
from . import datatype
from PIL.Image import Image


class PyJuizTypeInspectError(Exception): 
    def __init__(self, type_obj):
        self._type_obj = type_obj

    def __str__(self):
        return "【エラー】アノテーションのタイプ検出に失敗: {}(type_obj=%s)" % (self.__class__.__name__, self._type_obj)
    
class PyJuizCannotCreateDeafultError(PyJuizTypeInspectError):
    def __str__(self):
        return "【エラー】dataclassタイプのデフォルト値でのオブジェクト生成に失敗: {}(type_obj=%s)" % (self.__class__.__name__, self._type_obj)
    
class PyJuizUnknownTypeError(PyJuizTypeInspectError):
    def __str__(self):
        return "【エラー】アノテーションのタイプ検出に失敗。プリミティブでのdataclassでもありません。: {}(type_obj=%s)" % (self.__class__.__name__, self._type_obj)


class PyJuizUnknownTypeButNoClueError(Exception): pass

class PyJuizArgumentUnknownTypeButNoClueError(Exception):
    def __init__(self, arg_index):
        self._arg_index = arg_index
    def __str__(self):
        return "【エラー】{}番目の引数のタイプ推定に失敗。アノテーションもデフォルト値もありません。: {}" % (self._arg_index, self.__class__.__name__)
        
        
class PyJuizArgumentCannotCreateDefaultError(PyJuizCannotCreateDeafultError): pass
class PyJuizArgumentUnknownTypeError(PyJuizUnknownTypeError): pass
    
class PyJuizProcessArgumentUnknownTypeError(Exception): pass
class PyJuizProcessReturnsUnknownTypeError(Exception): 
    def __init__(self, returns_type):
        self._returns = returns_type

    def __str__(self):
        return "PyJuizProcessReturnsUnknownTypeError(returns_type=%s)" % self._returns

class PyJuizProcessReturnsCannotCreateDeafultError(Exception):
    def __init__(self, returns_type):
        self._returns = returns_type

    def __str__(self):
        return "PyJuizProcessReturnsCannotCreateDeafultError(returns_type=%s)" % self._returns
    
class PyJuizContainerArgumentUnknownTypeError(Exception): pass
class PyJuizContainerReturnsUnknownTypeError(Exception): pass

def _signature(function):
    """関数のシグネチャ。from __future__ import annotations で文字列になった型注釈も型にする"""
    try:
        return inspect.signature(function, eval_str=True)
    except (NameError, TypeError, SyntaxError):
        return inspect.signature(function)


def allow_no_arg_decorator(decorator_function):
    def wrapper(*args, **kwargs):
        
        if len(args) != 0 and callable(args[0]):
            decorated_function= args[0]
            return functools.wraps(decorated_function)(decorator_function(decorated_function))
        else:
            def _wrapper(decorated_function):
                return functools.wraps(decorated_function)(decorator_function(decorated_function, *args, **kwargs))
            return _wrapper
    return wrapper

class PyJuizMissingTypeAnnotationError(TypeError):
    """Process / ContainerProcess / Container の引数や戻り値に型注釈がない場合のエラー"""
    def __init__(self, function_name, arg_name=None):
        self.function_name = function_name
        self.arg_name = arg_name
        if arg_name is None:
            msg = "【エラー】{} の戻り値に型注釈がありません。(例: def {}(...) -> int:)".format(function_name, function_name)
        else:
            msg = "【エラー】{} の引数 {} に型注釈がありません。(例: {}: int)".format(function_name, arg_name, arg_name)
        super().__init__(msg)


def _require_arg_annotation(function_name, param):
    if param.annotation is inspect.Parameter.empty:
        raise PyJuizMissingTypeAnnotationError(function_name, param.name)


def _require_return_annotation(function_name, signature):
    if signature.return_annotation is inspect.Signature.empty:
        raise PyJuizMissingTypeAnnotationError(function_name)


def _type_inspect(annotation_obj, default_obj):
    """ 
    型注釈(必須)と既定値から (タイプオブジェクト、dataclassかどうかのフラグ、デフォルト値) を返す。
    型注釈がない場合は呼び出し側で PyJuizMissingTypeAnnotationError を出しておくこと。
    """
    if default_obj is inspect.Parameter.empty:
        return __type_inspect_from_annotation(annotation_obj)
    
    if annotation_obj == int: return (annotation_obj, False, default_obj)
    elif annotation_obj == float: return (annotation_obj, False, default_obj)
    elif annotation_obj == dict: return (annotation_obj, False, default_obj)
    elif annotation_obj == list: return (annotation_obj, False, default_obj)
    elif annotation_obj == bool: return (annotation_obj, False, default_obj)
    elif annotation_obj == str: return  (annotation_obj, False, default_obj)
    elif annotation_obj == Image: return (annotation_obj, False, default_obj)
    elif dataclasses.is_dataclass(annotation_obj):
        try:
            return (dict, True, dataclasses.asdict(default_obj))
        except:
            raise PyJuizCannotCreateDeafultError(default_obj)
    else:
        raise PyJuizUnknownTypeError(annotation_obj)

def __type_inspect_from_annotation(annotation_obj):
    """ 
    returns: (タイプオブジェクト、dataclassかどうかのフラグ、デフォルト値)
    """
    if annotation_obj == int: return (annotation_obj, False, 0)
    elif annotation_obj == float: return (annotation_obj, False, 0.0)
    elif annotation_obj == dict: return (annotation_obj, False, {})
    elif annotation_obj == list: return (annotation_obj, False, [])
    elif annotation_obj == bool: return (annotation_obj, False, False)
    elif annotation_obj == str: return  (annotation_obj, False, "")
    elif annotation_obj == Image: return (annotation_obj, False, None)
    elif dataclasses.is_dataclass(annotation_obj):
        try:
            return (dict, True, dataclasses.asdict(annotation_obj()))
        except:
            raise PyJuizCannotCreateDeafultError(annotation_obj)
    else:
        raise PyJuizUnknownTypeError(annotation_obj)


def _typed_type_name(cls):
    """生成した型の manifest の type_name (仕様 §6 の表。record は Object、enum は String)"""
    import enum
    return 'String' if issubclass(cls, enum.Enum) else 'Object'


def _add_typed_arg_manifest(manifest, param):
    """juiz-idl が生成した型の引数。manifest に型参照を載せ、既定値は辞書の表し方にする"""
    cls = param.annotation
    if param.default is inspect.Parameter.empty:
        default = None  # 既定値なし (仕様 §6)
    else:
        default = datatype.converter_of(cls).to_value(param.default, param.name)
    manifest.add_typed_arg(param.name, "", default, datatype.type_schema(cls), _typed_type_name(cls))


def _add_arg_manifest(manifest, function_name, param):
    """ 型注釈を必須として、引数の情報を manifest に追加する。
    juiz-idl が生成した型ならその型 (呼び出し時に辞書から変換する)、それ以外は None を返す。"""
    _require_arg_annotation(function_name, param)
    if datatype.is_juiz_type(param.annotation):
        _add_typed_arg_manifest(manifest, param)
        return param.annotation
    try:
        param_type, is_dataclass, param_default = _type_inspect(param.annotation, param.default)
    except PyJuizCannotCreateDeafultError as e:
        raise PyJuizArgumentCannotCreateDefaultError(e._type_obj)
    except PyJuizUnknownTypeError as e:
        raise PyJuizArgumentUnknownTypeError(e._type_obj)
    param_desc = ""
    if param_type == int:
        manifest.add_int_arg(param.name, param_desc, param_default)
    elif param_type == float:
        manifest.add_float_arg(param.name, param_desc, param_default)
    elif param_type == bool:
        manifest.add_bool_arg(param.name, param_desc, param_default)
    elif param_type == str:
        manifest.add_string_arg(param.name, param_desc, param_default)
    elif param_type == dict:
        manifest.add_object_arg(param.name, param_desc, param_default)
    elif param_type == list:
        manifest.add_array_arg(param.name, param_desc, param_default)
    elif param_type == Image:
        manifest.add_image_arg(param.name, param_desc, param_default)
    return None


def _returns_profile(returns):
    """ 戻り値の型注釈から (出力のプロファイル、dataclassかどうかのフラグ) を返す。"""
    if returns == int:
        return (PrimitiveProfile(type_name='Int', default_value=''), False)
    elif returns == float:
        return (PrimitiveProfile(type_name='Float', default_value=''), False)
    elif returns == bool:
        return (PrimitiveProfile(type_name='Bool', default_value=''), False)
    elif returns == str:
        return (PrimitiveProfile(type_name='String', default_value=''), False)
    elif returns == dict:
        return (PrimitiveProfile(type_name='Object', default_value=''), False)
    elif returns == list:
        return (PrimitiveProfile(type_name='Array', default_value=''), False)
    elif returns == Image:
        return (PrimitiveProfile(type_name='Array', default_value=''), False)
    elif dataclasses.is_dataclass(returns):
        try:
            sample_obj = dataclasses.asdict(returns())
        except:
            raise PyJuizProcessReturnsCannotCreateDeafultError(returns_type=returns)
        if isinstance(sample_obj, dict):
            return (PrimitiveProfile(type_name='Object', default_value=''), True)
        raise PyJuizProcessReturnsUnknownTypeError(returns_type=returns)
    else:
        raise PyJuizProcessReturnsUnknownTypeError(returns_type=returns)


def _set_returns(manifest, function_name, signature):
    """戻り値の型注釈を manifest に載せる。

    returns: (dataclassかどうかのフラグ, juiz-idl が生成した型か None)
    """
    _require_return_annotation(function_name, signature)
    returns = signature.return_annotation
    if datatype.is_juiz_type(returns):
        manifest.set_output_schema(datatype.type_schema(returns), _typed_type_name(returns))
        return False, returns
    manifest.outputs, is_dataclass = _returns_profile(returns)
    return is_dataclass, None


def _convert_args(names, arg_types, args, kwargs):
    """juiz-idl が生成した型の引数を、その型の値にする。

    同じ型の値 (Python の Process が返したネイティブ値) はそのまま、辞書 (ほかの言語・ブローカーから) は
    仕様 §4 の規則で読む (juiz.datatype.coerce)。
    """
    args = [a if t is None else datatype.coerce(t, a, n) for a, n, t in zip(args, names, arg_types)] + list(args[len(arg_types):])
    for k, v in list(kwargs.items()):
        if k in names:
            t = arg_types[names.index(k)]
            if t is not None:
                kwargs[k] = datatype.coerce(t, v, k)
    return args, kwargs


def _convert_result(returns_is_dataclass, returns_type, result):
    if returns_type is not None:
        # 生成した型の値はそのまま返す (Python 同士ではネイティブ値のまま渡り、ほかが読むときに to_value で辞書になる)
        return datatype.coerce(returns_type, result)
    if returns_is_dataclass:
        return convert_process_result(dataclasses.asdict(result))
    return convert_process_result(result)
    

class JuizProcess(object):
    def __init__(self, proc):
        self.__proc = proc
        self.name = proc.__name__
        self.signature = _signature(proc)
        self._manifest = ProcessManifest.new(self.name).set_description(proc.__doc__)
        self._arg_names = list(self.signature.parameters)
        self._arg_types = []
        
        for p in self.signature.parameters:
            self._arg_types.append(_add_arg_manifest(self._manifest, self.name, self.signature.parameters[p]))
        self._returns_is_dataclass, self._returns_type = _set_returns(self._manifest, self.name, self.signature)
        
    def manifest(self):
        return self._manifest.into_value()
        
    def __call__(self, *args, **kwargs):
        args, kwargs = _convert_args(self._arg_names, self._arg_types, args, kwargs)
        return _convert_result(self._returns_is_dataclass, self._returns_type, self.__proc(*args, **kwargs))
    
@allow_no_arg_decorator
def juiz_process(process_function, description="Default Description"):
    return JuizProcess(process_function)

class JuizContainer(object):
    def __init__(self, proc):
        self.__proc = proc
        self.name = proc.__name__
        self.signature = _signature(proc)
        self._manifest = ContainerManifest.new(self.name).set_description(proc.__doc__)
        self._arg_names = list(self.signature.parameters)
        self._arg_types = []
        for p in self.signature.parameters:
            self._arg_types.append(_add_arg_manifest(self._manifest, self.name, self.signature.parameters[p]))
    def manifest(self):
        return self._manifest.into_value()
        
    def __call__(self, *args, **kwargs):
        args, kwargs = _convert_args(self._arg_names, self._arg_types, args, kwargs)
        return self.__proc(*args, **kwargs)
    
@allow_no_arg_decorator
def juiz_container(constructor_function, description="Default Description"):
    return JuizContainer(constructor_function)

@allow_no_arg_decorator
def juiz_component_container(constructor_function, description="Default Description"):
    return JuizContainer(constructor_function)

class JuizContainerProcess(object):
    def __init__(self, proc, container_type):
        self.__proc = proc
        self.name = proc.__name__
        self.signature = _signature(proc)
        self._manifest = ProcessManifest.new(self.name).set_description(proc.__doc__).set_container_type(container_type)
        self._arg_names = list(self.signature.parameters)
        self._arg_types = []
        for i, p in enumerate(self.signature.parameters):
            if i == 0:
                self._arg_types.append(None)
                continue # 第1引数はコンテナ
            self._arg_types.append(_add_arg_manifest(self._manifest, self.name, self.signature.parameters[p]))
        self._returns_is_dataclass, self._returns_type = _set_returns(self._manifest, self.name, self.signature)
    def manifest(self):
        return self._manifest.into_value()
        
    def __call__(self, *args, **kwargs):
        args, kwargs = _convert_args(self._arg_names, self._arg_types, args, kwargs)
        return _convert_result(self._returns_is_dataclass, self._returns_type, self.__proc(*args, **kwargs))

def convert_process_result(retval):
    # if isinstance(retval, Image):
    #     output = io.BytesIO()
    #     retval.save(output, format='PNG')
    #     return output.getvalue() # Hex Data
    return retval

@allow_no_arg_decorator
def juiz_container_process(constructor_function, container_type:str, description="Default Description"):
    return JuizContainerProcess(constructor_function, container_type)

@allow_no_arg_decorator
def juiz_component_container_process(constructor_function, container_type:str, description="Default Description"):
    return JuizContainerProcess(constructor_function, container_type)