# juiz_idl

[juiz](https://github.com/OpenJUIZ/juiz) の IDL (`.jidl`) の生成器です。`.jidl` と手で書いた `.avsc` を読み、次のものを書き出します。

- `<完全名>.avsc`: 型ごとの Avro のスキーマ (juiz の型の正本)
- `<完全名>.schema.json`: 辞書での値の表し方の JSON Schema 2020-12
- `<namespace>.processes.json`: `process` / `container` 宣言のシグネチャ
- Rust: struct / enum と `juiz_sdk::types::JuizType` の実装

仕様は [docs/spec/datatype.md](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/datatype.md) の §9・§16 にあります。

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

## ライセンス

MIT または Apache-2.0 のどちらか (デュアルライセンス)。
