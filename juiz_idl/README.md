# juiz_idl

[juiz](https://github.com/OpenJUIZ/juiz) の IDL (`.jidl`) の生成器です。`.jidl` と手で書いた `.avsc` を読み、次のものを書き出します。

- `<完全名>.avsc`: 型ごとの Avro のスキーマ (juiz の型の正本)
- `<完全名>.schema.json`: 辞書での値の表し方の JSON Schema 2020-12
- `<namespace>.processes.json`: `process` / `container` 宣言のシグネチャ
- Rust: struct / enum と `juiz_sdk::types::JuizType` の実装
- Rust の雛形 (`-g rust-skeleton`): `process` / `container` 宣言から、本体が `todo!()` のコンポーネント `<namespace>.skeleton.rs`
- C++ (`-g cpp`): 名前空間ごとのヘッダ (`robot.geometry.h`) と全部を読み込む `juiz_idl.h` に struct / enum class と cppjuiz の `juiz::type_traits` の特殊化 (ヘッダのみ)
- Haskell (`-g haskell`): 名前空間ごとのモジュール (`Robot/Geometry.hs`) に record・直和型と hsjuiz の `JuizType` / `FromCapsule` / `ToCapsule` のインスタンス (Template Haskell なし)
- TypeScript (`-g typescript`): 名前空間ごとの `robot.geometry.ts` と `juiz_idl.ts` に interface・文字列リテラルの union と型参照 (管理画面・`bindings/js` 用、実行時の依存なし)
- Python (`-g python`): 名前空間ごとのパッケージ (`robot/geometry/__init__.py`) に dataclass / enum.Enum と、`process` / `container` 宣言の型付きクライアント (pyjuiz を使う)

仕様は [docs/spec/datatype.md](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/datatype.md) の §9・§16・§18・§20〜§24 にあります。

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

## TypeScript から使う

```sh
juiz-idl -g typescript -o gen idl/geometry.jidl   # gen/robot.geometry.ts と gen/juiz_idl.ts
```

```typescript
import { type Pose2D, Pose2DType } from "./gen/robot.geometry.js";

const target: Pose2D = { position: { x: 1, y: 0 }, theta: 0, frame: "map", comment: null };
await fetch(`/api/process/call?identifier=${id}`, {
  method: "PATCH",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ target }),
});
console.log(Pose2DType.fingerprint); // .avsc と同じ指紋
```

値は juiz が JSON で受け渡す辞書の形そのままです (`bytes` は base64 の文字列、`T?` は `T | null`)。

## 標準の型 juiz_std

`juiz_std/common.jidl` (`Time`・`Header` など)、`juiz_std/geometry.jidl` (`Point2D`・`Pose2D`・`Pose`・`Twist` など)、`juiz_std/sensor.jidl` (`ImageInfo`・`Imu`・`LaserScan`・`Range`) は juiz-idl に組み込まれていて、ファイルがなくても import や入力に書けます。

```text
namespace my.robot;
import idl "juiz_std/geometry.jidl";
record Goal { juiz_std.geometry.Pose2D pose; double tolerance = 0.1; }
```

Rust はクレート `juiz_std_types`、Python は pyjuiz に同梱の `juiz_std` パッケージに同じ型が入っています。

## import の探し先とほかのパッケージの型

- `-I <dir>` (build.rs は `.import_dir(dir)`、環境変数 `JUIZ_IDL_PATH` も) で import の探し先を足せます。相対パス → 探し先 → 組み込みの型の順に探します。
- `--extern <名前空間>[=<Rust のパス>]` (build.rs は `.extern_path(名前空間, Rust のパス)`) を付けると、その名前空間の型を書き出さず、ほかのパッケージにあるものを参照します。

```rust
// build.rs。juiz_std の型は juiz_std_types のものを使う
fn main() {
    juiz_idl::build::configure()
        .extern_path("juiz_std", "::juiz_std_types::juiz_std")
        .compile(&["idl/my_robot.jidl"])
        .unwrap();
}
```

```sh
juiz-idl -g python --extern juiz_std -o gen idl/my_robot.jidl   # juiz_std は pyjuiz のものを使う
```

## 型パッケージを配る

自分の型を Rust のクレートとして配り、ほかのクレートの JIDL から使えるようにするには、.jidl をクレートに入れて `links` で場所を知らせます。

```toml
# my_types/Cargo.toml
[package]
name = "my_types"
links = "my_types"
```

```rust
// my_types/build.rs
fn main() {
    juiz_idl::build::compile(&["idl/my_robot.jidl"]).unwrap();
    juiz_idl::build::export_idl_dir("idl").unwrap(); // 依存するクレートに DEP_MY_TYPES_IDL_DIR を渡す
}
```

```rust
// 使う側の build.rs (my_types に直接依存する)
fn main() {
    juiz_idl::build::configure()
        .import_dir(std::env::var("DEP_MY_TYPES_IDL_DIR").unwrap())
        .extern_path("my_robot", "::my_types::my_robot")
        .compile(&["idl/car.jidl"]) // import idl "my_robot.jidl"; と書ける
        .unwrap();
}
```

PyPI・CMake・Hackage での配り方は [docs/spec/datatype.md](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/datatype.md) の §24 にあります。

## ライセンス

MIT または Apache-2.0 のどちらか (デュアルライセンス)。
