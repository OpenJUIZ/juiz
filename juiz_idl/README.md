# juiz_idl

[juiz](https://github.com/OpenJUIZ/juiz) の IDL (`.jidl`) の生成器です。`.jidl` と手で書いた `.avsc` を読み、次のものを書き出します。

- `<完全名>.avsc`: 型ごとの Avro のスキーマ (juiz の型の正本)
- `<完全名>.schema.json`: 辞書での値の表し方の JSON Schema 2020-12
- `<namespace>.processes.json`: `process` / `container` 宣言のシグネチャ
- Rust: struct / enum と `juiz_sdk::types::JuizType` の実装
- Rust の雛形 (`-g rust-skeleton`): `process` / `container` 宣言から、本体が `todo!()` のコンポーネント `<namespace>.skeleton.rs`
- Python (`-g python`): 名前空間ごとのパッケージ (`robot/geometry/__init__.py`) に dataclass / enum.Enum と、`process` / `container` 宣言の型付きクライアント (pyjuiz を使う)

仕様は [docs/spec/datatype.md](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/datatype.md) の §9・§16・§18・§20 にあります。

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

## ライセンス

MIT または Apache-2.0 のどちらか (デュアルライセンス)。
