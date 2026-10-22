# Haskell の例

hsjuiz(`bindings/hsjuiz`)で書いた Haskell の Process / Container / Component です(issue #131, #132, #133, #134)。

- `src/IncrementProcess.hs`: `increment_process_hs`。arg1 に 1 を足す
- `src/DecrementProcess.hs`: `decrement_process_hs`。arg1 から 1 を引く(結果が IO でない関数の例)
- `src/ValueTypesProcess.hs`: `value_types_process_hs`。真偽値・整数・小数・文字列・配列・オブジェクトの引数を受け取り、オブジェクトを返す(juiz のテスト用)
- `src/ExampleContainer.hs`: `example_container_hs`。整数を一つ保持する Container。初期値は設定ファイルの `value`
- `src/ExampleContainerIncrement.hs`: `example_container_hs_increment`。Container の値に arg0 を足して返す ContainerProcess
- `src/ExampleContainerGet.hs`: `example_container_hs_get`。Container の値を返す ContainerProcess
- `src/ExampleContainerState.hs`: Container の状態の型。Container と ContainerProcess の両方のライブラリに入れる
- `src/MismatchedContainerProcess.hs`: `mismatched_container_process_hs`。状態の型が違う Container に結びつけたときにエラーになることを確かめる(juiz のテスト用)
- `src/ExampleComponent.hs`: `example_component_hs`。Process 2 つ(`example_component_hs_increment`、`example_component_hs_greet`)と
  Container 1 つ(`example_component_hs_counter`、ContainerProcess `_get` と `_increment`)を 1 つのライブラリにまとめた Component
- `src/ExampleComponentCounter.hs`: Component に入れる Container と ContainerProcess(要素を別のモジュールに書く例)
- `src/BrokenComponent.hs`: `broken_component_hs`。Component に無い Container の ContainerProcess を入れた誤った Component。読み込むときにエラーになることを確かめる(juiz のテスト用)

Process は `ProcessManifest` でマニフェストを書き、`juizProcess 'func 'manifest` で公開しています。
Container は `containerManifest` と `juizContainerWithDestroy 'create 'destroy 'manifest`、
ContainerProcess は `containerProcessManifest` と `juizContainerProcess 'func 'manifest` で公開しています。
Component は `componentManifest` と `juizComponent 'component [componentProcess 'func 'manifest, ...]` で公開しています。
書き方は `bindings/hsjuiz/README.md`、プラグインが公開する関数の決まりは `bindings/cjuiz/README.md` にあります。

## ビルド

GHC 9.6 以降と cabal が必要です(Linux / macOS)。リポジトリの直下で `cargo build` を済ませてから実行します。

```terminal
examples/haskell/build.sh
```

`target/libincrement_process_hs.so`、`target/libexample_container_hs.so`(macOS は `.dylib`)などができます。

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

### Container

```terminal
cargo run -- -d -f examples/haskell/example_container.conf
```

別の端末から HTTP の API で呼びます(`value: 15` で作った Container に 3 を足し、値を読む)。

```terminal
curl -X PATCH 'http://127.0.0.1:8000/api/container_process/call?identifier=core://core/container_process/hsinc0::example_container_hs_increment:hscont0' \
  -H 'content-type: application/json' -d '{"arg0": 3}'
# 18
curl -X PATCH 'http://127.0.0.1:8000/api/container_process/call?identifier=core://core/container_process/hsget0::example_container_hs_get:hscont0' \
  -H 'content-type: application/json' -d '{}'
# 18
```

### Component

```terminal
cargo run -- -d -f examples/haskell/example_component.conf
```

別の端末から呼びます(Component の Process と、`value: 15` で作った Container の ContainerProcess)。

```terminal
cargo run -- process call -p 'http://127.0.0.1:8000/process/hsinc0::example_component_hs_increment' '{"arg0": 41}'
# CapsulePtr((value:Value(42), option: []))
curl -X PATCH 'http://127.0.0.1:8000/api/process/call?identifier=core://core/process/hsgreet0::example_component_hs_greet' \
  -H 'content-type: application/json' -d '{"name": "juiz"}'
# "Hello, juiz!"
curl -X PATCH 'http://127.0.0.1:8000/api/container_process/call?identifier=core://core/container_process/hsinc1::example_component_hs_counter_increment:hscounter0' \
  -H 'content-type: application/json' -d '{"arg0": 3}'
# 18
```
