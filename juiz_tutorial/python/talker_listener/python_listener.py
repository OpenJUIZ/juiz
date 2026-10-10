#!/usr/bin/env python3
import juiz
from dataclasses import dataclass

@dataclass
class OutputObject:
    hello: int = 0
    key: str = 'Hello'

@juiz.juiz_process
def python_listener(arg1: str="Hello, Juiz!") -> OutputObject:
    print(f'listener: "{arg1}"')
    return arg1

