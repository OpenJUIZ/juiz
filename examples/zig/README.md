# Zig の例

`language: "zig"` のプラグインの例です(issue #572)。利用者向けのライブラリ zigjuiz(issue #573 以降)を使わず、
C ヘッダ `bindings/cjuiz/include/juiz_sdk.h` を build.zig の `addTranslateC` で Zig のモジュール `c` にして、
juiz が探す C の関数(`manifest_entry_point`、`process_factory_entry_point`)を手で書いています。

| ファイル | 内容 |
|---|---|
| `src/increment_process_zig.zig` | arg1 に 1 を足す Process |
| `increment_process.conf` | この例を読み込む設定ファイル |

## ビルド

Zig 0.17.x が必要です(juiz が対応する Zig は 1 つの版に固定しています。違う版では build.zig がエラーにします)。
先にリポジトリの直下で `cargo build` を実行して `target/debug` に libjuiz_sdk を作ってから、次を実行します。

```sh
examples/zig/build.sh          # zig build して ../../target に写す
```

zig コマンドの名前は環境変数 `ZIG` で変えられます(pip の `ziglang` パッケージなら `ZIG="python3 -m ziglang"`)。
build.sh を使わずに `zig build --release -Djuiz-sdk-lib=<libjuiz_sdk のあるディレクトリの絶対パス>` でもビルドできます(`zig-out/lib` にできます)。

## 動かす

リポジトリの直下で、HTTP ブローカー付きで起動します。

```terminal
cargo run -- -d -f examples/zig/increment_process.conf
```

別の端末から呼びます。

```terminal
cargo run -- process call -p 'http://127.0.0.1:8000/process/ziginc0::increment_process_zig' '{"arg1": 41}'
# CapsulePtr((value:Value(42), option: []))
```
