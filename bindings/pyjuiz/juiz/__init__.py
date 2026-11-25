
# from . import proxy
from .juiz import *

from .decorators import juiz_process, juiz_container, juiz_container_process, juiz_component_container, juiz_component_container_process
from .decorators import PyJuizMissingTypeAnnotationError
from . import datatype
# 名前で juiz を引く (docs/spec/naming.md §9.4、issue #618)
from .naming import resolve_name, name_list, NameNotRegisteredError, NameAmbiguousError
# Python の ContainerProcess から ContainerPose を読み書きする (issue #371)
from .transformations import pose, set_pose, lookup, set_transform, set_model, clear_model, ContainerModelInvalidError, TransformationsError, NoCommonAnchorError, PoseNotWritableError, InvalidPoseValueError, TransformationsNotAttachedError, TransformNotWritableError, TransformNotFoundError
from .datatype import ValueDoesNotMatchTypeError
# from .decorators import JuizContainer, JuizContainerProcess, JuizProcess

def __getattr__(name):
    # 古い名前 PrimitiveProfile 等 (#506)。juiz.juiz の __getattr__ に任せる
    from . import juiz as _juiz_module
    if name in _juiz_module._DEPRECATED_NAMES:
        return _juiz_module._deprecated_name(name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
