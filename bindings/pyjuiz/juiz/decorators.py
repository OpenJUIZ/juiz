import datetime, dataclasses, traceback, io
import inspect
import functools

from .juiz import *
from PIL.Image import Image
import docstring_parser


class PyJuizDocStringParseError(Exception):  pass

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

def _type_inspect(annotation_obj, default_obj):
    if annotation_obj is inspect._empty and default_obj is inspect._empty:
        raise PyJuizUnknownTypeButNoClueError()
    if annotation_obj is inspect._empty:
        return __type_inspect_from_default(default_obj)
    elif default_obj is inspect._empty:
        return __type_inspect_from_annotation(annotation_obj)
    
    if annotation_obj == int: return (annotation_obj, False, default_obj)
    elif annotation_obj == float: return (annotation_obj, False, default_obj)
    elif annotation_obj == dict: return (annotation_obj, False, default_obj)
    elif annotation_obj == list: return (annotation_obj, False, default_obj)
    elif annotation_obj == bool: return (annotation_obj, False, default_obj)
    elif annotation_obj == str: return  (annotation_obj, False, default_obj)
    elif annotation_obj == Image: return (annotation_obj, False, default_obj)
    elif dataclasses.is_dataclass(annotation_obj):
        return (dict, True, dataclasses.asdict(default_obj))
    else:
        raise PyJuizUnknownTypeError()
    
def __type_inspect_from_default(default_obj):
    """ 
    returns: (タイプオブジェクト、dataclassかどうかのフラグ、デフォルト値)
    """
    if isinstance(default_obj, int): return (int, False, default_obj)
    elif isinstance(default_obj, bool): return (bool, False, default_obj)
    elif isinstance(default_obj, float): return (float, False, default_obj)
    elif isinstance(default_obj, list): return (list, False, default_obj)
    elif isinstance(default_obj, str): return (str, False, default_obj)
    elif isinstance(default_obj, dict): return (dict, False, default_obj)
    elif dataclasses.is_dataclass(default_obj):
        try:
            return (dict, True, dataclasses.asdict(default_obj))
        except:
            raise PyJuizCannotCreateDeafultError(default_obj)
    else:
        raise PyJuizUnknownTypeError(default_obj)

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
        raise PyJuizUnknownTypeError()
    

class JuizProcess(object):
    def __init__(self, proc):
        self.__proc = proc
        self.name = proc.__name__
        self.signature = inspect.signature(proc)
        try:
            self._doc_obj = docstring_parser.parse(proc.__doc__)
            print('doc_obj:', self._doc_obj)
        except:
            raise PyJuizDocStringParseError()
        
        self._manifest = ProcessManifest.new(self.name).set_description(proc.__doc__)
        self._arg_is_dataclass = []
        self._returns_is_dataclass = False
        
        for arg_index, p in enumerate(self.signature.parameters):
            self.__add_arg_manif(arg_index, self.signature.parameters[p])
        self.__add_returns_manif(self.signature.return_annotation)
            
    def __add_returns_manif(self, returns):
        if returns == int:
            self._manifest.outputs = PrimitiveProfile(type_name='Int', default_value='')
        elif returns == float:
            self._manifest.outputs = PrimitiveProfile(type_name='Float', default_value='')
        elif returns == bool:
            self._manifest.outputs = PrimitiveProfile(type_name='Bool', default_value='')
        elif returns == str:
            self._manifest.outputs = PrimitiveProfile(type_name='String', default_value='')
        elif returns == dict:
            self._manifest.outputs = PrimitiveProfile(type_name='Object', default_value='')
        elif returns == list:
            self._manifest.outputs = PrimitiveProfile(type_name='Array', default_value='')
        elif returns == Image:
            self._manifest.outputs = PrimitiveProfile(type_name='Array', default_value='')
        else:
            if dataclasses.is_dataclass(returns):
                try:
                    sample_obj = dataclasses.asdict(returns())
                except:
                    raise PyJuizProcessReturnsCannotCreateDeafultError(returns_type=returns)
                if isinstance(sample_obj, dict):
                    self._manifest.outputs = PrimitiveProfile(type_name='Object', default_value='')
                    self._returns_is_dataclass = True
                else:
                    raise PyJuizProcessReturnsUnknownTypeError(returns_type=returns)
            else:
                raise PyJuizProcessReturnsUnknownTypeError(returns_type=returns)
        
    def __add_arg_manif(self, arg_index, param):
        try:
            param_type, is_dataclass, param_default = _type_inspect(param.annotation, param.default)
            print('param:', param)
            param_desc = ""
            self._arg_is_dataclass.append(is_dataclass)
            if param_type == int:
                self._manifest.add_int_arg(param.name, param_desc, param_default)
            elif param_type == float:
                self._manifest.add_float_arg(param.name, param_desc, param_default)
            elif param_type == bool:
                self._manifest.add_bool_arg(param.name, param_desc, param_default)
            elif param_type == str:
                self._manifest.add_string_arg(param.name, param_desc, param_default)
            elif param_type == dict:
                self._manifest.add_object_arg(param.name, param_desc, param_default)
            elif param_type == list:
                self._manifest.add_array_arg(param.name, param_desc, param_default)
            elif param_type == Image:
                self._manifest.add_image_arg(param.name, param_desc, param_default)
            
        except PyJuizUnknownTypeButNoClueError as e:
            raise PyJuizArgumentUnknownTypeButNoClueError(arg_index)
        except PyJuizCannotCreateDeafultError as e:
            raise PyJuizArgumentCannotCreateDefaultError(e._type_obj)
        except PyJuizUnknownTypeError as e:
            raise PyJuizArgumentUnknownTypeError(e._type_obj)
        
        
    def manifest(self):
        return self._manifest.into_value()
        
    def __call__(self, *args, **kwargs):
        if self._returns_is_dataclass:
            return convert_process_result(dataclasses.asdict(self.__proc(*args, **kwargs)))
        else:
            return convert_process_result(self.__proc(*args, **kwargs))
    
@allow_no_arg_decorator
def juiz_process(process_function, description="Default Description"):
    return JuizProcess(process_function)

class JuizContainer(object):
    def __init__(self, proc):
        self.__proc = proc
        self.name = proc.__name__
        self.signature = inspect.signature(proc)
        self._manifest = ContainerManifest.new(self.name).set_description(proc.__doc__)
        for p in self.signature.parameters:
            param = self.signature.parameters[p]
            param_type = param.annotation
            param_default = param.default
            if param_type is inspect._empty:
                if isinstance(param_default, int):
                    param_type = int
                elif isinstance(param_default, bool):
                    param_type = bool
                elif isinstance(param_default, float):
                    param_type = float
                elif isinstance(param_default, list):
                    param_type = list
                elif isinstance(param_default, str):
                    param_type = str
                elif isinstance(param_default, dict):
                    param_type = dict
                elif isinstance(param_default, Image):
                    param_type = Image
                else:
                    raise PyJuizProcessArgumentUnknownTypeError()
            if param_default is inspect._empty:
                if param_type == int:
                    param_default = 0
                elif param_type == float:
                    param_default = 0.0
                elif param_type == dict:
                    param_default = {}
                elif param_type == list:
                    param_default = []
                elif param_type == bool:
                    param_default = False
                elif param_type == str:
                    param_default = ""
                elif param_type == Image:
                    param_default =  None
                else:
                    raise PyJuizProcessArgumentUnknownTypeError()
                
            if param_type == int:
                self._manifest.add_int_arg(param.name, "", param_default)
            elif param_type == float:
                self._manifest.add_float_arg(param.name, "", param_default)
            elif param_type == bool:
                self._manifest.add_bool_arg(param.name, "", param_default)
            elif param_type == str:
                self._manifest.add_string_arg(param.name, "", param_default)
            elif param_type == dict:
                self._manifest.add_object_arg(param.name, "", param_default)
            elif param_type == list:
                self._manifest.add_array_arg(param.name, "", param_default)
            elif param_type == Image:
                self._manifest.add_image_arg(param.name, "", param_default)
    def manifest(self):
        return self._manifest.into_value()
        
    def __call__(self, *args, **kwargs):
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
        self.signature = inspect.signature(proc)
        self._manifest = ProcessManifest.new(self.name).set_description(proc.__doc__).set_container_type(container_type)
        for i, p in enumerate(self.signature.parameters):
            if i == 0:
                continue
            param = self.signature.parameters[p]
            param_type = param.annotation
            param_default = param.default
            if param_type is inspect._empty:
                if isinstance(param_default, int):
                    param_type = int
                elif isinstance(param_default, bool):
                    param_type = bool
                elif isinstance(param_default, float):
                    param_type = float
                elif isinstance(param_default, list):
                    param_type = list
                elif isinstance(param_default, str):
                    param_type = str
                elif isinstance(param_default, dict):
                    param_type = dict
                elif isinstance(param_default, Image):
                    param_type = Image
                else:
                    raise PyJuizProcessArgumentUnknownTypeError()
            if param_default is inspect._empty:
                if param_type == int:
                    param_default = 0
                elif param_type == float:
                    param_default = 0.0
                elif param_type == dict:
                    param_default = {}
                elif param_type == list:
                    param_default = []
                elif param_type == bool:
                    param_default = False
                elif param_type == str:
                    param_default = ""
                elif param_type == Image:
                    param_default = None
                else:
                    print('Param Type Unknown:', param_type)
                    raise PyJuizProcessArgumentUnknownTypeError()
                
            # print('p:', param.name, param.default, param.kind, param.annotation, param)
            if param_type == int:
                self._manifest.add_int_arg(param.name, "", param_default)
            elif param_type == float:
                self._manifest.add_float_arg(param.name, "", param_default)
            elif param_type == bool:
                self._manifest.add_bool_arg(param.name, "", param_default)
            elif param_type == str:
                self._manifest.add_string_arg(param.name, "", param_default)
            elif param_type == dict:
                self._manifest.add_object_arg(param.name, "", param_default)
            elif param_type == list:
                self._manifest.add_array_arg(param.name, "", param_default)
            elif param_type == Image:
                self._manifest.add_image_arg(param.name, "", param_default)
    def manifest(self):
        return self._manifest.into_value()
        
    def __call__(self, *args, **kwargs):
        return convert_process_result(self.__proc(*args, **kwargs))

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