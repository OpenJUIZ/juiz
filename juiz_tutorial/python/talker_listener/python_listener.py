#!/usr/bin/env python3
import juiz

@juiz.juiz_process
def python_listener(arg1: str="Hello, Juiz!"):
    print(f'listener: "{arg1}"')
    return arg1

