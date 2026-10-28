# Python のコンポーネント (issue #221)。component_manifest() を書かず、デコレータの付いた
# Process・Container・ContainerProcess を 1 つのファイルに並べる
from juiz import juiz_process, juiz_container, juiz_container_process


class PyCompAccumulator:
    def __init__(self, value):
        self.value = value


@juiz_container
def py_comp_acc(initial_value: int = 0):
    return PyCompAccumulator(initial_value)


@juiz_container_process(container_type="py_comp_acc")
def py_comp_acc_add(container, x: int = 1) -> int:
    container.value = container.value + x
    return container.value


@juiz_process
def py_comp_double(x: int = 1) -> int:
    return x * 2
