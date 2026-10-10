from juiz import juiz_container

class PyCounter:
    def __init__(self, value):
        self.value = value

@juiz_container
def py_counter(initial_value: int = 0):
    return PyCounter(initial_value)
