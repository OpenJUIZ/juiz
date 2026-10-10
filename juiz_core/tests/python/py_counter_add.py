from juiz import juiz_container_process

@juiz_container_process(container_type="py_counter")
def py_counter_add(container, arg1: int = 1) -> int:
    container.value = container.value + arg1
    return container.value
