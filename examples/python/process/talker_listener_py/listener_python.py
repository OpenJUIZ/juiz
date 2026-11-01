#!/usr/bin/env python3
import juiz

@juiz.juiz_process
def listener_python(arg1: str="Hello, Juiz!") -> str:
    print(f'listener_python receives "{arg1}"')
    return arg1

