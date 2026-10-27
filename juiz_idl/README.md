# juiz_idl

[juiz](https://github.com/OpenJUIZ/juiz) の IDL (`.jidl`) の生成器です。`.jidl` と手で書いた `.avsc` を読み、次のものを書き出します。

- `<完全名>.avsc`: 型ごとの Avro のスキーマ (juiz の型の正本)
- `<完全名>.schema.json`: 辞書での値の表し方の JSON Schema 2020-12
- `<namespace>.processes.json`: `process` / `container` 宣言のシグネチャ
- Rust: struct / enum と `juiz_sdk::types::JuizType` の実装
- Rust の雛形 (`-g rust-skeleton`): `process` / `container` 宣言から、本体が `todo!()` のコンポーネント `<namespace>.skeleton.rs`

仕様は [docs/spec/datatype.md](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/datatype.md) の §9・§16・§18 にあります。

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

## ライセンス

MIT または Apache-2.0 のどちらか (デュアルライセンス)。
