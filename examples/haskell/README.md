# Haskell の例

hsjuiz(`bindings/hsjuiz`)で書いた Haskell の Process です(issue #131, #132)。

- `src/IncrementProcess.hs`: `increment_process_hs`。arg1 に 1 を足す
- `src/DecrementProcess.hs`: `decrement_process_hs`。arg1 から 1 を引く(結果が IO でない関数の例)
- `src/ValueTypesProcess.hs`: `value_types_process_hs`。真偽値・整数・小数・文字列・配列・オブジェクトの引数を受け取り、オブジェクトを返す(juiz のテスト用)

どれも `ProcessManifest` でマニフェストを書き、`juizProcess 'func 'manifest` で公開しています。
書き方は `bindings/hsjuiz/README.md`、プラグインが公開する関数の決まりは `bindings/cjuiz/README.md` にあります。

## ビルド

GHC 9.6 以降と cabal が必要です(Linux / macOS)。リポジトリの直下で `cargo build` を済ませてから実行します。

```terminal
examples/haskell/build.sh
```

`target/libincrement_process_hs.so`(macOS は `.dylib`)などができます。

## 動かす

リポジトリの直下で、HTTP ブローカー付きで起動します。

```terminal
cargo run -- -d -f examples/haskell/increment_process.conf
```

別の端末から呼びます。

```terminal
cargo run -- process call -p 'http://127.0.0.1:8000/process/hsinc0::increment_process_hs' '{"arg1": 41}'
# CapsulePtr((value:Value(42), option: []))
```
