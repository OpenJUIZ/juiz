# juiz_idl

[juiz](https://github.com/OpenJUIZ/juiz) の IDL (`.jidl`) の生成器です。`.jidl` と手で書いた `.avsc` を読み、次のものを書き出します。

- `<完全名>.avsc`: 型ごとの Avro のスキーマ (juiz の型の正本)
- `<完全名>.schema.json`: 辞書での値の表し方の JSON Schema 2020-12
- `<namespace>.processes.json`: `process` / `container` 宣言のシグネチャ
- Rust: struct / enum と `juiz_sdk::types::JuizType` の実装
- Rust の雛形 (`-g rust-skeleton`): `process` / `container` 宣言から、本体が `todo!()` のコンポーネント `<namespace>.skeleton.rs`
- C++ (`-g cpp`): 名前空間ごとのヘッダ (`robot.geometry.h`) と全部を読み込む `juiz_idl.h` に struct / enum class と cppjuiz の `juiz::type_traits` の特殊化 (ヘッダのみ)
- Haskell (`-g haskell`): 名前空間ごとのモジュール (`Robot/Geometry.hs`) に record・直和型と hsjuiz の `JuizType` / `FromCapsule` / `ToCapsule` のインスタンス (Template Haskell なし)
- Python (`-g python`): 名前空間ごとのパッケージ (`robot/geometry/__init__.py`) に dataclass / enum.Enum と、`process` / `container` 宣言の型付きクライアント (pyjuiz を使う)

仕様は [docs/spec/datatype.md](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/datatype.md) の §9・§16・§18・§20〜§22 にあります。

## コマンド

```sh
cargo install juiz_idl
juiz-idl -o out -g avsc,json-schema,processes,rust idl/geometry.jidl
```

`juiz` コマンド (juiz_app) からは `juiz idl …` で同じことができます。

## build.rs から使う

```toml
[dependencies]
juiz_sdk = "0.0"

[build-dependencies]
juiz_idl = "0.0"
```

```rust
// build.rs
fn main() {
    juiz_idl::build::compile(&["idl/geometry.jidl"]).unwrap();
}
```

```rust
// src/lib.rs。名前空間 robot.geometry の型は robot::geometry::Pose2D になる
include!(concat!(env!("OUT_DIR"), "/juiz_idl.rs"));
```

## Process の雛形を作る

```sh
juiz-idl -o src -g rust-skeleton idl/geometry.jidl   # src/robot.geometry.skeleton.rs ができる
```

できたファイルを `src/lib.rs` などにコピーして `todo!()` の本体を書きます。型は `crate::robot::geometry::Pose2D` のように参照するので、クレートの一番上で `juiz_idl.rs` を読み込んでおきます。雛形は一度作って手で直していくものなので、build.rs では作りません。

## Python から使う

```sh
juiz-idl -o gen -g python idl/geometry.jidl   # gen/robot/__init__.py, gen/robot/geometry/__init__.py ができる
```

`gen` を `sys.path` (juiz の conf の `option.pythonpath` など) に入れて使います。

```python
from juiz import juiz_process
from robot.geometry import Pose2D, MoveToClient

@juiz_process
def move_to(target: Pose2D, speed: float = 1.0) -> Pose2D:   # 引数は Pose2D のまま届く
    ...

pose = MoveToClient.connect('move_to0', port=8000)(Pose2D(...))  # 型付きクライアント
```

## C++ から使う

CMake の関数 `juiz_generate_types()` (juiz の `bindings/cppjuiz/cmake/juiz_idl.cmake`) が、ビルドのときにヘッダを作ります。

```cmake
include(${JUIZ_HOME}/bindings/cppjuiz/cmake/juiz_idl.cmake)
juiz_generate_types(my_component IDL idl/geometry.jidl DEPENDS idl/common.jidl)
```

```cpp
#include "juiz/juiz.h"
#include "juiz_idl.h"
using robot::geometry::Pose2D;

static auto move_to_manif = ProcessManifest("move_to")
    .add_typed_arg<Pose2D>("target", "")
    .add_float_arg("speed", "", 1.0)
    .returns<Pose2D>();
std::optional<Pose2D> move_to(const Pose2D& target, double speed) { ... }
COMPONENT_PROCESS_FACTORY(move_to_manif, move_to);
```

戻り値の record は辞書にせず C++ の値のまま渡り、同じコンパイラでビルドした C++ の Process はそのまま (複製せずに) 受け取ります。

## Haskell から使う

```sh
juiz-idl -g haskell -o gen idl/geometry.jidl   # gen/Robot/Geometry.hs など
```

cabal の `hs-source-dirs` に `gen` を足し、`other-modules` に `Robot.Geometry` などを書きます。

```haskell
import Juiz
import Robot.Geometry

moveToManifest :: ProcessManifest
moveToManifest = processManifest "move_to"
  & addTypedArgNoDefault (Proxy :: Proxy Pose2D) "target" ""
  & addFloatArg "speed" "" 1.0

moveTo :: Pose2D -> Double -> IO Pose2D   -- output_schema は結果の型から載る
```

フィールドの関数は型名 + フィールド名 (`pose2DPosition`、`point2DX` など)、列挙の構築子は型名 + 記号 (`MotorModeIdle`) です。

## ライセンス

MIT または Apache-2.0 のどちらか (デュアルライセンス)。
