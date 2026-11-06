<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
    <img src="assets/logo-light.svg" alt="juiz" width="320">
  </picture>
</p>

# juiz - the robot middleware

English | [日本語](README.md)

[![Linux](https://github.com/OpenJUIZ/juiz/actions/workflows/test-linux.yml/badge.svg?branch=main)](https://github.com/OpenJUIZ/juiz/actions/workflows/test-linux.yml?query=branch%3Amain)
[![macOS](https://github.com/OpenJUIZ/juiz/actions/workflows/test-macos.yml/badge.svg?branch=main)](https://github.com/OpenJUIZ/juiz/actions/workflows/test-macos.yml?query=branch%3Amain)
[![Windows](https://github.com/OpenJUIZ/juiz/actions/workflows/test-windows.yml/badge.svg?branch=main)](https://github.com/OpenJUIZ/juiz/actions/workflows/test-windows.yml?query=branch%3Amain)
[![Python](https://github.com/OpenJUIZ/juiz/actions/workflows/python.yml/badge.svg?branch=main)](https://github.com/OpenJUIZ/juiz/actions/workflows/python.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/OpenJUIZ/juiz/badges/coverage.json)](https://github.com/OpenJUIZ/juiz/actions/workflows/test-linux.yml)

JUIZ is a middleware and software platform for building robots and IoT devices as networked distributed systems.
You can easily write software that drives cameras, robots, and robot elements such as motors and sensors in several languages, and combine the pieces so that they run as a single software service.

**Official site: https://openjuiz.github.io/** ([日本語](https://openjuiz.github.io/ja/))

> [!NOTE]
> juiz is developed by a single person, ysuga. Please do not use it for mission-critical tasks.

## Features

- **Process**: You write software as idempotent functions that have inputs (arguments) and an output.
- **Container**: When you need to keep state, you use a Container, and read and write the state through ContainerProcesses bound to it.
- **Two ways to use it**: You can connect Processes and drive a robot by data flow (closer to robot middleware such as ROS), or call a Process from outside and get the result (client-server style development).
- **Multiple languages**: Functional elements can be written in Rust, C++, and Python.
- **Broker**: HTTP+JSON and QUIC brokers are provided out of the box, connecting Processes in separate processes or on separate machines.
- **Execution Context**: Execution contexts such as periodic execution (TimerEC) and execution on the main thread (MainLoopEC) are provided.

## Quick start

Write a Process in Python that adds 1 to its argument, then call it from outside.
All you need is Python 3.10 or later; no Rust toolchain is required (Linux, macOS, Windows).

`pip install juiz-cli` installs both the `juiz` command and pyjuiz, which you use to write Processes.

```sh
python3 -m venv .venv
source .venv/bin/activate        # On Windows: .venv\Scripts\activate
pip install juiz-cli
```

Create `increment_process.py`. The file name and the function name must match.

```python
from juiz import juiz_process


@juiz_process
def increment_process(arg1: int = 0) -> int:
    return arg1 + 1
```

Load the Process, create one instance of it, and wait (Ctrl+C to quit).

```sh
juiz --process ./increment_process.py -l python -1 -d
```

In another terminal, activate the same venv and call it.

```sh
juiz process call "http://127.0.0.1:8000/process/increment_process0::increment_process" '{"arg1": 3}' -p
# => CapsulePtr((value:Value(4), option: []))
```

While it is waiting, juiz also serves a management UI for the browser. Open the URL shown at startup (http://localhost:8000/ui/ by default) to see the processes and connections in the system as a diagram ([webui/README.md](webui/README.md)).

### Writing in Rust or C++

To write in Rust or C++, install the `juiz` command with `cargo install juiz_app` (Rust 1.89 or later is required).
The `juiz-cli` package on pip is built on CI, and it cannot load Rust plugins you build locally because the compiler versions do not match.

If you also use Python Processes with this `juiz`, add **only** `pip install pyjuiz`. If you install `juiz-cli` as well, you end up with two `juiz` commands, and which one runs depends on the order of your PATH.

```sh
cargo install juiz_app
pip install pyjuiz   # if you also use Python Processes
```

How to write in Rust and C++, and how to connect Processes, is explained in the tutorial on the official site.

## Documentation

| | English | 日本語 |
|---|---|---|
| Installation | [Install](https://openjuiz.github.io/getting-started/install/) | [インストール](https://openjuiz.github.io/ja/getting-started/install/) |
| Tutorial (Process, Container, configuration files, connect, Component, connecting separate processes, QUIC) | [Tutorial](https://openjuiz.github.io/tutorial/) | [チュートリアル](https://openjuiz.github.io/ja/tutorial/) |
| How it works | [Concepts](https://openjuiz.github.io/concepts/overview/) | [コンセプト](https://openjuiz.github.io/ja/concepts/overview/) |
| How-to guides (Process, Container, configuration files) | [Guides](https://openjuiz.github.io/guides/process/) | [ガイド](https://openjuiz.github.io/ja/guides/process/) |

- Examples: [Rust](https://github.com/OpenJUIZ/juiz_examples_rust) / [Python](https://github.com/OpenJUIZ/juiz_examples_python) / [C++](https://github.com/OpenJUIZ/juiz_examples_cpp)
- API reference: [juiz_sdk (docs.rs)](https://docs.rs/juiz_sdk)
- Packages: [juiz-cli (PyPI)](https://pypi.org/project/juiz-cli/) / [juiz_app (crates.io)](https://crates.io/crates/juiz_app) / [pyjuiz (PyPI)](https://pypi.org/project/pyjuiz/)
- Why juiz was made and its design philosophy (in Japanese): [MOTIVATION.md](MOTIVATION.md)
- Building from source, testing, development environment, repository layout (in Japanese): [CONTRIBUTING.md](CONTRIBUTING.md)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
