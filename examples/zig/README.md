# Zig の例

`language: "zig"` のプラグインの例です。Zig 向けのライブラリ zigjuiz([bindings/zigjuiz](../../bindings/zigjuiz/README.md)、issue #573)で書いています。
zigjuiz は `build.zig.zon` の `dependencies` に、リポジトリの `bindings/zigjuiz` を `.path` で入れています。

| ファイル | 内容 |
|---|---|
| `src/increment_process_zig.zig` | arg1 に 1 を足す Process(i64 の最大値なら error.Overflow をエラーで返す) |
| `src/greet_process_zig.zig` | `juiz.Context` の arena で文字列を組み立てて返す Process |
| `src/echo_types_process_zig.zig` | zigjuiz で使える引数の型を全部受け取り、辞書(`juiz.Value`)にして返す Process |
| `src/fail_process_zig.zig` | 自分で定義したエラーを返す Process |
| `src/example_container_state.zig` | Container の状態の型 `Counter`(Container と ContainerProcess の両方が `@import` する、issue #574) |
| `src/example_container_zig.zig` | 整数を一つ保持する Container。`Counter.deinit` は Container を削除するときに呼ばれる |
| `src/example_container_zig_increment.zig` | Container の値に arg0 を足す ContainerProcess |
| `src/example_container_zig_get.zig` | Container の値を返す ContainerProcess。状態を `*const` で受けるので readonly |
| `src/mismatched_container_process_zig.zig` | 状態を別の型で受ける ContainerProcess(型違いのエラー -1104 を確かめるテスト用) |
| `src/zig_part*.zig` | ContainerPose を読み書きする Container `zig_part` と ContainerProcess(`juiz.pose` / `setPose` / `lookup` / `setTransform`) |
| `increment_process.conf` | increment_process_zig を読み込む設定ファイル |
| `example_container.conf` | example_container_zig と ContainerProcess を読み込む設定ファイル |

juiz のテスト(`juiz_core/tests/integration/plugins/zig/`、`juiz_core/tests/integration/transformations/zig.rs`)は、
これらを読み込んで値の読み書き・Container の状態・ContainerPose・エラーを確かめます。
1 ライブラリに複数入れる Component は issue #575 で足すので、いまは ContainerProcess ごとに別のライブラリです。

## ビルド

Zig 0.17.x が必要です(juiz が対応する Zig は 1 つの版に固定しています。違う版では zigjuiz の build.zig がエラーにします)。
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

Container は `examples/zig/example_container.conf` で起動し、ContainerProcess を呼びます。

```terminal
cargo run -- -d -f examples/zig/example_container.conf
cargo run -- container-process call -p 'http://127.0.0.1:8000/container_process/ziginc0::example_container_zig_increment:zigcont0' '{"arg0": 3}'
# CapsulePtr((value:Value(18), option: []))
```
