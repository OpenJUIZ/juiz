#!/usr/bin/env python3
import juiz

@juiz.juiz_process
def python_talker():
    print('talker_python says "Hello, World!"')
    return "Hello, World!"

