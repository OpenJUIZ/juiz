import sys
sys.path.append('.')
sys.path.append('..')

import inspect

from juiz import *

@juiz_process
def increment_process(arg1:int, arg2=3.0) -> int:
    """_summary_

    Args:
        arg1 (int, optional): _description_. Defaults to 3.
        arg2 (float, optional): _description_. Defaults to 3.0.

    Returns:
        _type_: _description_
    """
    print('pyadd ', arg1, '>', arg1+1)
    return arg1 + 1


v = increment_process(3)
print('v=', v)

print('manifest = ', increment_process.manifest())