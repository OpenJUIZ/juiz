<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
    <img src="assets/logo-light.svg" alt="juiz" width="320">
  </picture>
</p>

# juiz - the robot middleware

[![Test](https://github.com/OpenJUIZ/juiz/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/OpenJUIZ/juiz/actions/workflows/test.yml)
[![Python](https://github.com/OpenJUIZ/juiz/actions/workflows/python.yml/badge.svg?branch=main)](https://github.com/OpenJUIZ/juiz/actions/workflows/python.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/OpenJUIZ/juiz/badges/coverage.json)](https://github.com/OpenJUIZ/juiz/actions/workflows/test.yml)

JUIZ (ジュイス) は、ロボットやIoT機器をネットワーク分散システムとして開発するためのミドルウェアおよびソフトウェアプラットフォームです。
カメラやロボット、モータやセンサなどのロボット要素を動かすソフトウェアを、複数の言語で簡単に開発し、それらを結合して一つのソフトウェアサービスのように動作させることができます。

**公式サイト: https://openjuiz.github.io/ja/** ([English](https://openjuiz.github.io/))

> [!NOTE]
> juiz は ysuga ひとりで開発しているソフトウェアです。ミッションクリティカルなタスクへの利用は控えてください。

## 特徴

- **Process**: 入力（引数）と出力を持つ冪等な関数としてソフトウェアを開発します。
- **Container**: 状態を保持する場合は Container を使い、Container に結びついた ContainerProcess で状態を読み書きします。
- **2つの使い方**: Process 同士を connect してデータフロー的にロボットを動かすこと（ROS などのロボット用ミドルウェア寄りの開発）も、外部から Process を call して結果を得ること（サーバー・クライアント的な開発）もできます。
- **多言語対応**: 機能要素は Rust、C++、Python で実装できます。
- **Broker**: HTTP+JSON と QUIC のブローカーを標準で提供し、別々のプロセスやマシンの Process をつなげます。
- **Execution Context**: 周期実行 (TimerEC) やメインスレッドでの実行 (MainLoopEC) などの実行コンテキストを提供します。

## すぐに試す

引数に 1 を足して返す Process を Python で書いて、外から呼び出してみます。
Python 3.10 以降があれば、Rust のツールチェーンは要りません(Linux・macOS・Windows)。

`pip install juiz-cli` で、`juiz` コマンドと、Process を書くための pyjuiz の両方が入ります。

```sh
python3 -m venv .venv
source .venv/bin/activate        # Windows では .venv\Scripts\activate
pip install juiz-cli
```

`increment_process.py` を作ります。ファイル名と関数名はそろえます。

```python
from juiz import juiz_process


@juiz_process
def increment_process(arg1: int = 0) -> int:
    return arg1 + 1
```

Process を読み込んで、インスタンスを 1 つ作って待機します（Ctrl+C で終了）。

```sh
juiz --process ./increment_process.py -l python -1 -d
```

別のターミナルで同じ venv を有効にしてから呼び出します。

```sh
juiz process call "http://127.0.0.1:8000/process/increment_process0::increment_process" '{"arg1": 3}' -p
# => CapsulePtr((value:Value(4), option: []))
```

待機中の juiz はブラウザ用の管理画面も配信しています。起動時に出る URL(既定は http://localhost:8000/ui/)を開くと、系の中の処理や接続を図で確かめられます([webui/README.md](webui/README.md))。

### Rust・C++ で書く場合

Rust や C++ で書く場合は、`juiz` コマンドを `cargo install juiz_app` で入れます(Rust 1.81 以降が必要です)。
pip の `juiz-cli` は CI でビルドしたもので、手元でビルドした Rust のプラグインはコンパイラの版がそろわないため読み込めません。

この `juiz` で Python の Process も使うときは、`pip install pyjuiz` **だけ** を追加してください。`juiz-cli` も入れると `juiz` コマンドが 2 つになり、どちらが動くかが PATH の順番で決まってしまいます。

```sh
cargo install juiz_app
pip install pyjuiz   # Python の Process も使う場合
```

Rust と C++ で書く方法や、Process をつなぐ方法は、公式サイトのチュートリアルで説明しています。

## ドキュメント

| | 日本語 | English |
|---|---|---|
| インストール | [インストール](https://openjuiz.github.io/ja/getting-started/install/) | [Install](https://openjuiz.github.io/getting-started/install/) |
| チュートリアル（Process、Container、設定ファイル、connect、Component、別プロセス接続、QUIC） | [チュートリアル](https://openjuiz.github.io/ja/tutorial/) | [Tutorial](https://openjuiz.github.io/tutorial/) |
| しくみ | [コンセプト](https://openjuiz.github.io/ja/concepts/overview/) | [Concepts](https://openjuiz.github.io/concepts/overview/) |
| 書き方の逆引き（Process、Container、設定ファイル） | [ガイド](https://openjuiz.github.io/ja/guides/process/) | [Guides](https://openjuiz.github.io/guides/process/) |

- サンプル集: [Rust](https://github.com/OpenJUIZ/juiz_examples_rust) / [Python](https://github.com/OpenJUIZ/juiz_examples_python) / [C++](https://github.com/OpenJUIZ/juiz_examples_cpp)
- API リファレンス: [juiz_sdk (docs.rs)](https://docs.rs/juiz_sdk)
- パッケージ: [juiz-cli (PyPI)](https://pypi.org/project/juiz-cli/) / [juiz_app (crates.io)](https://crates.io/crates/juiz_app) / [pyjuiz (PyPI)](https://pypi.org/project/pyjuiz/)
- juiz を作った動機と設計思想: [MOTIVATION.md](MOTIVATION.md)
- ソースからのビルド、テスト、開発環境、リポジトリ構成: [CONTRIBUTING.md](CONTRIBUTING.md)

## ライセンス

juiz は、次のどちらかのライセンスを選んで利用できます(デュアルライセンス)。

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) または <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) または <https://opensource.org/licenses/MIT>)

Licensed under either of Apache License, Version 2.0 or MIT license at your option.

### コントリビューション

明示的に別の意思表示をしない限り、このリポジトリに提出されたコントリビューションは、Apache-2.0 ライセンスの定義に従い、追加の条件なしに上記と同じデュアルライセンスで提供されるものとします。

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
