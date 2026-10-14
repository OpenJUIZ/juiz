from juiz import juiz_container_process

@juiz_container_process(
    container_type="example_container_python"
)
def example_container_python_get(container) -> int:
    # print(f'example_container_python_get({container}) called')
    return container.value
