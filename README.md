# juiz - the robot middleware

[![Test](https://github.com/sugarsweetrobotics/juiz/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/sugarsweetrobotics/juiz/actions/workflows/test.yml)
[![Python](https://github.com/sugarsweetrobotics/juiz/actions/workflows/python.yml/badge.svg?branch=main)](https://github.com/sugarsweetrobotics/juiz/actions/workflows/python.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/sugarsweetrobotics/juiz/badges/coverage.json)](https://github.com/sugarsweetrobotics/juiz/actions/workflows/test.yml)

JUIZ (ジュイス) は、ロボットやIoT機器をネットワーク分散システムとして開発するためのミドルウェアおよびソフトウェアプラットフォームです。
カメラやロボット、モータやセンサなどのロボット要素を動かすソフトウェアを、複数の言語で簡単に開発し、それらを結合して一つのソフトウェアサービスのように動作させることができます。

> [!NOTE]
> juiz は ysuga ひとりで開発しているソフトウェアです。ミッションクリティカルなタスクへの利用は控えてください。

juiz を作った動機と設計思想については [MOTIVATION.md](MOTIVATION.md) にまとめています。

## 特徴

- **Process**: 入力（引数）と出力を持つ冪等な関数としてソフトウェアを開発します。
- **Container**: 状態を保持する場合は Container を使い、Container に結びついた ContainerProcess で状態を読み書きします。
- **2つの使い方**: Process 同士を connect してデータフロー的にロボットを動かすこと（ROS などのロボット用ミドルウェア寄りの開発）も、外部から Process を call して結果を得ること（サーバー・クライアント的な開発）もできます。
- **多言語対応**: 機能要素は Rust、C++、Python で実装できます。
- **Broker**: HTTP+JSON と QUIC のブローカーを標準で提供します。HTTP ブローカーは OpenAPI 定義を提供するので、Swagger UI から動作確認できます。
- **Execution Context**: 周期実行 (TimerEC) やメインスレッドでの実行 (MainLoopEC) などの実行コンテキストを提供します。

## 必要環境

- Rust 1.81 以降
- Python 3 と開発用ヘッダー（Python 連携に PyO3 を使うため。Ubuntu なら `python3-dev`）
- C++ の機能要素をビルドする場合は CMake と C++17 対応のコンパイラ

## ビルドとインストール

```terminal
git clone https://github.com/sugarsweetrobotics/juiz.git
cd juiz
cargo build
```

ビルドすると `target/debug/juiz` コマンドとサンプルの機能要素が生成されます。
`juiz` コマンドをインストールする場合は以下を実行します。

```terminal
cargo install --path juiz_app
```

Python で機能要素を書く場合は pyjuiz パッケージをインストールするか、`bindings/pyjuiz` に PYTHONPATH を通してください。

```terminal
pip install ./bindings/pyjuiz
```

C++ のサンプルは CMake でビルドします。

```terminal
cmake -S examples/cpp -B examples/cpp/build
cmake --build examples/cpp/build
```

### VS Code + Dev Containers で開発する

`.devcontainer/` に、CI と同じ環境(Ubuntu 24.04、Rust stable、CMake、Clang、Python 3 と pyjuiz の依存パッケージ)のコンテナ設定があります。
Docker と VS Code の Dev Containers 拡張機能を入れた状態でリポジトリを開き、コマンドパレットから「Dev Containers: Reopen in Container」を選ぶとコンテナ内で開発できます。
`PYO3_PYTHON` と `PYTHONPATH` は設定済みなので、コンテナ内のターミナルで CI と同じ手順でビルドとテストができます。

```terminal
cargo build
cmake -S examples/cpp -B examples/cpp/build -DCMAKE_BUILD_TYPE=Debug
cmake --build examples/cpp/build --parallel
cargo test -- --test-threads=1
```

HTTP ブローカーの 8000 番ポートはホストに転送されるので、ホストのブラウザから http://localhost:8000/docs を開けます。

`.vscode/` には、コンテナの内外どちらでも使える VS Code の設定があります。

- `extensions.json`: 推奨拡張機能(rust-analyzer、CodeLLDB など。devcontainer と同じもの)
- `settings.json`: 保存時の clippy チェック、rust-analyzer のビルド先の分離(`target/rust-analyzer`)など
- `tasks.json`: CI と同じ手順のビルド・テスト(「Tasks: Run Build Task」「Tasks: Run Test Task」)
- `launch.json`: CodeLLDB で `juiz` コマンドや juiz_core の単体テストをデバッグ実行する設定

## クイックスタート

設定ファイル（YAML 形式の .conf ファイル）を `-f` オプションで指定して `juiz` コマンドを起動します。

```terminal
juiz -f examples/rust/container/example_container.conf -d
```

`-d` オプションを付けると実行後に待機します（Ctrl+C で終了）。
待機中はデフォルトで 8000 番ポートで HTTP ブローカーが動作しているので、ブラウザで以下にアクセスすると Swagger UI から API を試せます。

```
http://127.0.0.1:8000/docs
```

## リポジトリ構成

| ディレクトリ | 内容 |
|---|---|
| `juiz_sdk` | 機能要素を開発するための crate |
| `juiz_macro` | `#[juiz_process]` などのマクロ |
| `juiz_core` | 機能要素を実体化・管理するコア (CoreBroker, Broker など) |
| `juiz_app` | `juiz` コマンド |
| `brokers` | 追加のブローカー実装 (QUIC など) |
| `ecs` | 実行コンテキスト (TimerEC, OneShotEC, MainLoopEC) |
| `bindings` | C++ (`cppjuiz`)、Python (`pyjuiz`)、JavaScript 向けバインディング |
| `examples` | Rust / C++ / Python のサンプル |
| `juiz_tutorial` | チュートリアル |

## 機能要素の実装方法
機能要素であるContainer, ContainerProcessおよびProcessは、Rust, Python, C++の３種類の言語で実装することができる。

### Processの実装
#### Rustでの実装

機能要素を実装するには、juiz_sdkというcrateを使う。Cargo.tomlは以下のようになる。
```toml
[package]
name = "increment_process"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
juiz_sdk = { path = "$PATH_TO/juiz_sdk/" }
```
$PATH_TOにはjuiz_sdk crateまでの相対パスを書く。 (これはcrates.ioにjuiz_sdkを登録したら楽になると思う。)

例えば、引数に1を足して返すだけの純粋プロセスのコードを書いてみる。

Rustで記述するのが現状ではもっともエレガントにProcessやContainerを記述できる。

``` rust
use juiz_sdk::prelude::*;

#[juiz_process]
fn increment_process(arg1: i64) -> JuizResult<Capsule> {
    log::trace!("increment_process({:?}) called", arg1);
    return Ok(jvalue!(arg1+1).into());
}
```
まずjuiz_sdk::prelude::*をインポートすると、基本的なマクロや変数の型が使えるようになる。
juiz_processマクロを当てた関数が、Processの本体になる。関数の名前がProcessのタイプ名になる。
引数は複数の引数が使えて、i64, f64, bool, String, Value, Vec<Value>などが使える。
引数の名前もパラメータになっている。
juiz_processマクロに引数を与えると、ドキュメントやデフォルト引数を自動生成できる。詳しい内容は後述（予定）

#### C++での実装

モジュールのローダーであるjuizコマンドはrustで書かれているが、他の言語とのインターフェースを持っているので、機能モジュールを別の言語で書くことができる。
C++では、exportすべき関数の名前と、扱うべきデータ型が決まっており、これを提供するヘッダーファイルであるjuiz.hが提供されている。
juiz.hはbindings/cppjuiz/includeディレクトリにあるので、このディレクトリにINCLDUE_PATHを通しておいてほしい。

``` c++
#include "juiz/juiz.h"

juiz::Value manifest() {
    return ProcessManifest{"increment_process_cpp"}
        .add_int_arg("arg1", "test_argument", 1)
        .into_value();
}

std::optional<int64_t> increment_process(juiz::CapsuleMap cm) {
    auto a = cm.get_int("arg1");
    return a + 1;
}

PROCESS_FACTORY(manifest, increment_process);
```
C++はRustで自動生成していた部分をかなり自分で書かないといけない。
これはいずれなんとかしたいが、できるのだろうか・・・

#### Pythonでの実装

PythonとのインターフェースはRustのPyO3 crateを用いて実装されており、入出力で扱うデータ型は主にintやstrなどのプリミティブやlist, tuple, dictなどの複合型になる。
独自のデータ型を使う場合は、dataclassを使って構成して、juizに渡す関数の出力ではasdictメソッドでdictに変換して送ることになる。

pythonはpyjuizというパッケージを作成してある。
bindings/pyjuizにPYTHONPATHを通しておくと便利だ。

``` python
from juiz import *

@juiz_process
def increment_process(arg1:int = 1):
    return arg1 + 1
```
Pythonはデコレータで記述量をかなり減らすことができた。

### Containerの実装
Containerはstructを与えてやることで実現する。
後述のContainerProcessはこのstructを最初の引数として受け取るProcessを定義することになる。

#### Rustでの実装
例によってRustでのContainerの記述はエレガントである。
Cargo.tomlについてはProcessの章を参照してほしい。

Containerを作成する関数にjuiz_containerのマクロアトリビュートを追加するだけで実現できる。
この関数をコンテナのコンストラクタと呼ぶことにする。
返り値はBoxして渡して欲しい。
``` rust
use juiz_sdk::prelude::*;

#[repr(Rust)]
pub struct ExampleContainer {
    pub value: i64
}

#[juiz_container]
fn example_container(initial_value: i64) -> JuizResult<Box<ExampleContainer>> {
    Ok(Box::new(ExampleContainer{value:initial_value}))
}
```

#### C++での実装
C++ではヘッダーファイル (*.h) でstructを定義して、ソースファイル (*.cpp) でコンストラクタ等を定義する。
ヘッダーファイルのPATHについてはProcessの章を参照してほしい。
``` c++ 
// -- example_contaienr.h
#pragma once

#include <cstdint>

class CppContainer {
public:
    int64_t value;
    CppContainer(int64_t v) : value(v) {}
};
```

コンテナのコンストラクタとしてcreate_container関数を定義している。
このあたりもtemplateを使えばもう少しウマく書けそうなんだけど、Rustとの接続の部分も含めて設計が必要で、難しい。
``` c++
// --- example_container_cpp.cpp
#include "juiz/juiz.h"
#include "example_container.h"

juiz::Value manifest() {
    return ContainerManifest("example_container_cpp").into_value();
}

CppContainer* create_container(juiz::Value value) {
    int64_t int_value = 0;
    if (value.isObjectValue()) {
        if (value.hasKey("value")) {
            auto objv = value.objectValue();
            auto v = objv["value"];
            if (v.isIntValue()) {
               int_value = v.intValue();
            }
        }   
    }
    return new CppContainer(int_value);
}

bool destroy_container(CppContainer* p_container) {
    if (p_container) {
        delete p_container;
        return true;
    }
    return false;
}

CONTAINER_FACTORY(manifest, create_container, destroy_container);
```
#### Pythonでの実装

Pythonはやはり記述としては少ないが、もう少しスッキリさせるにはデコレータでなんとかしたいと考えている。
pyjuizのPATHについてはProcessの章を参照してほしい。
juiz_containerデコレータで記述がスッキリした。
``` python

from juiz import *

class PyContainer:
    value: int
    def __init__(self, value):
        self.value = value

@juiz_container
def example_container_python(initial_value:int = 0):
    # print(f'example_container_python(value = {initial_value}) called')
    return PyContainer(initial_value)

```

### Container Processの実装

#### Rustでの実装
コンテナプロセスではjuiz_container_processマクロを使い、このマクロの引数に「container_type = ほにゃらら」という値を入れる。
マクロの引数は初めて出てきたが、実はjuiz_processやjuiz_containerにも引数を与えることができる。
Cargo.tomlについてはProcessの章を参照してほしい。
``` rust
use example_container::ExampleContainer;
use juiz_sdk::prelude::*;

#[juiz_container_process( container_type = "example_container" )]
fn increment_function(container: &mut ContainerImpl<ExampleContainer>, arg1: i64) -> JuizResult<Capsule> {
    container.value = container.value + arg1;
    return Ok(jvalue!(container.value).into());
}
```

#### C++での実装
C++はやはりどこか冗長な記述になってしまう。
コンテナを生成するコードで使ったヘッダーを再利用することで、同じ構造体にアクセスするコンテナプロセスを作ることができる。
juiz.hについてはProcessの章を参照してほしい。
``` c++
#include "juiz/juiz.h"
#include "example_container.h"

juiz::Value manifest() {
    return ProcessManifest("example_container_cpp_increment")
        .container_type("example_container_cpp")
        .add_int_arg("arg0", "test_argument", 2)
        .into_value();
}

std::optional<int64_t> example_container_increment(CppContainer* container, juiz::CapsuleMap cm) {
    int64_t v = cm.get_int("arg0");
    container->value = container->value + v;
    return container->value;
}

CONTAINER_PROCESS_FACTORY(CppContainer, manifest, example_container_increment)
```
#### Pythonでの実装
コンテナプロセスはやはりC++よりはスッキリと書ける。


``` python
from juiz import juiz_container_process

@juiz_container_process(
    container_type="example_container_python"
)
def example_container_python_get(container):
    # print(f'example_container_python_get({container}) called')
    return container.value
```

### Componentの実装
コンポーネントは、Process, Container, ContainerProcessを一つのプロジェクトで一斉に作り配布する方法である。


#### RustでのComponentの実装方法
```rust
use juiz_sdk::prelude::*;

#[juiz_component_process]
fn example_component_increment(arg1: i64) -> JuizResult<Capsule> {
    log::trace!("increment_process({:?}) called", arg1);
    return Ok(jvalue!(arg1+1).into());
}

#[repr(Rust)]
pub struct ExampleComponentContainer {
    pub value: i64
}

#[juiz_component_container]
fn example_component_container(initial_value: i64) -> JuizResult<Box<ExampleComponentContainer>> {
    println!("example_component_container({initial_value}) called");
    Ok(Box::new(ExampleComponentContainer{value: initial_value}))
}

#[juiz_component_container_process( container_type = "example_component_container" )]
fn example_component_container_get(container: &mut ContainerImpl<ExampleComponentContainer>) -> JuizResult<Capsule> {
    println!("example_component_container_get()");
    Ok(jvalue!(container.value).into())
}

#[juiz_component_container_process( container_type = "example_component_container" )]
fn example_component_container_increment(container: &mut ContainerImpl<ExampleComponentContainer>) -> JuizResult<Capsule> {
    println!("example_component_container_increment()");
    container.value = container.value + 1;
    Ok(jvalue!(container.value).into())
}   

#[juiz_component_container_process( container_type = "example_component_container" 
   arguments = {
      default = {
        arg1 = 1
      }
   }
)]
fn example_component_container_add(container: &mut ContainerImpl<ExampleComponentContainer>, arg1: i64) -> JuizResult<Capsule> {
    println!("example_component_container_add({arg1})");
    container.value = container.value + arg1;
    Ok(jvalue!(container.value).into())
}

juiz_component_manifest!(
    component_name = "example_component"
    containers = {
        example_component_container = [
            example_component_container_get,
            example_component_container_increment,
            example_component_container_add
        ]
    }
    processes = [
        example_component_increment
    ]
);
```

#### C++でのComponentの実装


## 機能要素の単体実行方法
juizクレートをビルドするとjuizコマンドが生成される。このコマンドを使う。

### Processを試す。
たとえば、RustでつくったProcessがtarget/debug/libtalker.dylibだった場合、以下のコマンドで単体プロセスが実行できる。
```terminal
juiz --process target/debug/libtalker.dylib -l rust -e -1
```
--processオプションで生成物を指定する。Pythonなら.pyファイル、C++ならば.dllや.so, .dylibなどのバイナリである。
-lオプションは言語を指定する。rust|cpp|pythonの3つから選び、デフォルトはrustであるので例の場合は"-l rust"は省略が可能なオプションである。
-eオプションはロードしたプロセスを一つ、自動で名前をつけて実体化し、デフォルトの引数を使ってexecuteする。
-1オプションで、ロードしたモジュール一つにつき、一つのインスタンスを作成する。

このjuizクレートの中で試すなら、以下のように行う
```terminal
cargo run -- --process ./target/debug/libtalker.dylib -e -1
```
デフォルトのbinとしてjuizコマンドが登録されているのでcargo runで実行される。

-dオプションをつけると、実行後にサーバーを立ててそのまま待機をする。
この状態ならばhttp://localhost:8000/docsをブラウザで開くと、swagger-uiを使ったテストを行うことができる。

### ContainerおよびContainerProcessを試す。
Rustで作ったContainerとContainerProcessがそれぞれ、./target/debug/my_container.dylibと./target/debug/my_container_process.dylibであった場合、以下のコマンドで単体のコンテナプロセスを実行できる。
```terminal
juiz --container ./target/debug/my_container.dylib --container_process ./target/debug/my_container_process.dylib -l rust -e -1
```
--containerでコンテナのファイルを、--container_processでコンテナプロセスのバイナリを指定する。
-lで言語を指定するのも同じであり、-eオプションも同じ効果である。
-1オプションで、ロードしたモジュール一つにつき、一つのインスタンスを作成する。
この方法ではただ一のコンテナのみインスタンスにできる。コンテナプロセスは、そのコンテナのプロセスとして結びつけられる。


## 設定ファイルの中身
複数の成果物を一気に読み込む場合は設定ファイルを記述するのが簡単である。

設定ファイルの例について示す。
``` yaml
name: "test_system"
option:
  http_broker:
    start: true
    port: 8000
plugins:  
  container_factories:
    example_container:
      language: "rust"
      path: "./target/debug"
      processes:
        example_container_get:
          path: "./target/debug"
        example_container_increment:
          path: "./target/debug"
  "process_factories":
    "increment_process":
      "path": "./target/debug"
"containers":
  - "type_name": "example_container"
    "name": "container0"
    "processes":
    - "type_name": "example_container_increment"
      "name": "increment0"
    - "type_name": "example_container_get"
      "name": "get0"
"processes":
  - "type_name": "increment_process"
    "name": "inc0" 
```
現状、かなり記述量が多いので、これを減らすことを考えている。

このファイルではコンテナのファクトリーとして、example_container型のコンテナのファクトリーを含んだexample_container.dylibファイルと、そのexample_containerに結び付けられたexample_container_get型のコンテナプロセスのファクトリーを含んだdylibファイル、同じくexample_container下のexample_container_incrementのdylib、
同時に、純粋プロセスのファクトリーとしてincrement_processのdylibを読み込んでいる。
ファクトリーのdylibファイルは、それが提供する型の名前＋拡張子、で指定するルールになっている。
また、containerの実体としてexample_container型のcontainer0を実体化し、このコンテナのメンバーとしてexample_container_increment型のコンテナプロセスであるincrement0と、example_container_getのget0を実体化している。
また、純粋プロセスであるincrement_process型のinc0も実体化している。

ここでもう少し設定ファイルについて説明する。

トップレベルの「name」はシステムの名前を定義する。

「option」はデフォルトで動作するモジュールの動作定義をする。
「http_broker」はhttp Brokerの振る舞いについて定義できる。
「start」をtrueにするとデフォルトでhttp_brokerが起動し、portで指定するポートで通信が可能になる。
これ以外にも後述するpythonpathなど、デフォルトの動作について調整できる。

「plugins」は、コンテナやプロセスおよびbrokerの実装のDLLを読み込むための定義が書かれている。
「container_factories」はコンテナのDLLの読み込み、「process_factories」はプロセスのDLL読み込みを行っている。

トップレベルの「containers」は、pluginsで読み込まれたコンテナを実体化するための設定が書かれている。
同様に「processes」は純粋プロセス実体化のための定義が書かれている。
