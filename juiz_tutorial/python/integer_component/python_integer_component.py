from juiz import *

class PyCompContainer:
    def __init__(self, arg:int = 0):
        self.buf = arg
        
@juiz_container
def python_integer_container(initial_value:  int = 0):
    return PyCompContainer(initial_value)

@juiz_container_process(
    container_type="python_integer_container"
)
def python_integer_container_set(pycomp, value:int = 0):
    pycomp.buf = value
    return pycomp.buf

@juiz_container_process(
    container_type="python_integer_container"
)
def python_integer_container_get(pycomp):
    return pycomp.buf

# def component_manifest():
#     return {
#         "type_name": "python_integer_component",
#         "containers": [
#             {
#                 "type_name": "python_integer_container",
#                 "processes": [
#                     {
#                         "type_name": "set",
#                         "arguments": {
#                             "arg0": {
#                                 "type": "int",
#                                 "description": "test_argument",
#                                 "default": 1,
#                             }
#                         }
#                     },
#                     {
#                         "type_name": "get",
#                         "arguments": {
#                         }
#                     },
#                 ]
#             }
#         ]
#     }