import time

from juiz import juiz_container_process


@juiz_container_process(container_type="py_part")
def py_part_sleep(part, sleep_ms: int = 0) -> int:
    """readonly でない ContainerProcess。sleep_ms だけ待つ (並行に走らないことの比較用)"""
    time.sleep(sleep_ms / 1000.0)
    return sleep_ms
