from juiz import juiz_container_process

@juiz_container_process(container_type="py_counter")
def py_counter_get(container) -> int:
    return container.value
