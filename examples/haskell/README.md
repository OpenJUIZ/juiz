# Haskell の例

手書きの FFI だけで書いた、最小の Haskell の Process です(issue #131)。
`increment_process_hs` は arg1 に 1 を足し、`decrement_process_hs` は 1 を引きます。

- `src/JuizFFI.hs`: libjuiz_sdk の C 関数(`bindings/cjuiz/include/juiz_sdk.h`)を `foreign import ccall` で呼ぶための宣言
- `src/IncrementProcess.hs`, `src/DecrementProcess.hs`: `manifest_entry_point` と `process_factory_entry_point` を `foreign export ccall` で公開する
- `cbits/juiz_hs_init.c`: juiz が最初に呼ぶ RTS の初期化関数 `juiz_hs_init`

プラグインが公開する関数の決まりは `bindings/cjuiz/README.md` にあります。
利用者向けの Haskell ライブラリ(hsjuiz)は issue #132 以降で用意します。

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
