# juiz の開発に参加する

juiz 本体をソースからビルドし、テストを実行するための情報です。
juiz を使ってロボットやIoT機器のソフトウェアを開発する方法は、[公式サイト](https://openjuiz.github.io/ja/)を見てください。

## 必要環境

- Rust 1.85 以降(Python 連携に使う PyO3 0.30 の要件)
- Python 3.10 以降と開発用ヘッダー（Python 連携に PyO3 を使うため。Ubuntu なら `python3-dev`）。CI では 3.10 と 3.14 で確かめています
- C++ の機能要素をビルドする場合は CMake と C++17 対応のコンパイラ

## ビルドとインストール

```terminal
git clone https://github.com/OpenJUIZ/juiz.git
cd juiz
cargo build
```

ビルドすると `target/debug/juiz` コマンドとサンプルの機能要素が生成されます。
`juiz` コマンドをインストールする場合は以下を実行します。

```terminal
cargo install --path juiz_app
```

### Swagger UI を使う

HTTP ブローカーは Swagger UI（`/docs`）を提供します（juiz_core と juiz_app の feature `swagger-ui`、既定でオン）。
OpenAPI 定義は `/api-docs/openapi.json` で取得できます。
Swagger UI が要らない場合は `--no-default-features` を付け、必要な feature（`embed-python` など）だけを指定してビルドします。

```terminal
cargo install --path juiz_app --no-default-features --features embed-python,qmp-broker
```

### QUIC (qmp) ブローカー

QUIC (qmp) ブローカー(`brokers/qmp_broker`、クレート `juiz_qmp_broker`)は juiz_app の feature `qmp-broker`(既定でオン)で `juiz` コマンドに組み込まれます(issue #63)。設定ファイルの `brokers` / `broker_proxies` に `type_name: qmp` を書くだけで使え、`plugins.broker_factories` でプラグインを読み込む必要はありません。
プラグインとしてビルドした `libqmp_broker.so` などを `plugins.broker_factories.qmp_broker` で読み込んだ場合は、そちらが優先されます。`path` は他のプラグインと同じく設定ファイルのあるディレクトリからの相対で書きます(0.0.27 まではブローカーと EC のプラグインだけカレントディレクトリからの相対でした、issue #284)。
自分で `System` を作るアプリケーションでは、`setup()` の前に `qmp_broker::register()` を呼ぶと組み込みで使えます。

### Python 連携の feature

Python の Process などを読み込む機能は、juiz_core の feature で切り替えます。

| feature | 内容 |
|---|---|
| `embed-python`(既定) | juiz の中で Python インタプリタを起動する。ネイティブバイナリ(juiz_app)向けで、libpython にリンクする |
| `extension-module` | juiz を Python の拡張モジュールとして Python プロセスの中で動かす(pip で入れる juiz 向け、issue #88)。libpython にはリンクしない。`default-features = false` と一緒に使う |
| `python` | Python プラグインの読み込みだけを有効にする(上の2つが内部で使う) |

Python を使わないなら、次のように既定の feature を外すと libpython なしで起動するバイナリになります。このとき Python のプラグインを読み込もうとするとエラーになります（`--features swagger-ui,qmp-broker` は Swagger UI と QUIC ブローカーを残すためのものです）。

```terminal
cargo install --path juiz_app --no-default-features --features swagger-ui,qmp-broker
```

Python で機能要素を書く場合は pyjuiz パッケージをインストールするか、`bindings/pyjuiz` に PYTHONPATH を通してください。

```terminal
pip install ./bindings/pyjuiz
```

### pip で入れる juiz(bindings/juiz_py)

`bindings/juiz_py` は、Rust のツールチェーンなしで `pip install juiz-cli` だけで `juiz` コマンドを使えるようにする PyPI パッケージ `juiz-cli` です(`juiz` は既存の `julz` と紛らわしいとして PyPI に登録できない)(issue #88)。juiz_app の feature `python-entry` と juiz_core の `extension-module` を使った abi3 の拡張モジュールを [maturin](https://www.maturin.rs/) でビルドします。Python から `import juiz` で使うデコレータは pyjuiz にあるので、拡張モジュールは `juiz_cli` という名前にしています。ルートのワークスペースには入れていません(ワークスペースをまとめてビルドすると `embed-python` と `extension-module` が同時に有効になるため)。

```terminal
pip install maturin ./bindings/pyjuiz
cd bindings/juiz_py
maturin build --release
pip install target/wheels/juiz_cli-*.whl
python tests/smoke_test.py                          # juiz_examples_python のディレクトリを渡すとサンプルも動かす
```

CI では `.github/workflows/juiz-wheels.yml` が Linux(x86_64・aarch64、manylinux)、macOS(arm64・x86_64)、Windows(x86_64)の wheel をビルドし、それぞれでスモークテストと [juiz_examples_python](https://github.com/OpenJUIZ/juiz_examples_python) のサンプルを動かします。
v* タグを打つと `.github/workflows/pypi-publish.yml` が pyjuiz を公開したあと、これらの wheel を PyPI の `juiz-cli` として公開します。そのため版上げでは、`bindings/juiz_py/pyproject.toml` の `version` と `pyjuiz==` の版、`bindings/juiz_py/Cargo.toml` の `version` もタグに揃えてください(揃っていないと公開しません)。

C++ のサンプルは CMake でビルドします。

```terminal
cmake -S examples/cpp -B examples/cpp/build
cmake --build examples/cpp/build
```

Haskell のサンプル(`examples/haskell`)は cabal でビルドします。GHC 9.6 以降と cabal が必要です(Linux / macOS)。
先に `cargo build` で `target/debug` に libjuiz_sdk を作ってから実行すると、できた共有ライブラリが `target/` に置かれます。

```terminal
examples/haskell/build.sh
```

### OpenCV のコンポーネントをビルドする

`examples/rust/component/juiz_opencv` は OpenCV を使うため、通常の `cargo build` には含まれません(`default-members` の外)。
opencv クレートがビルド時に libclang で OpenCV のヘッダを読むので、OpenCV の開発パッケージと libclang が必要です。Ubuntu では次のようにします。

```terminal
sudo apt-get install libopencv-dev clang libclang-dev
cargo build -p juiz_opencv
cargo test -p juiz_opencv    # カメラや画面を使わない処理(画像の変換、色変換、ファイルの読み書き)のテスト
```

CI では `.github/workflows/test-linux.yml` の `opencv` ジョブ(Ubuntu)で同じことを実行しています。

### テストを実行する

`scripts/test.sh` が、テストの前提になる準備(ワークスペースの `cargo build`、C++ の例の cmake ビルド、Haskell の例の cabal ビルド、Python の例が使うパッケージの確認と `PYTHONPATH=bindings/pyjuiz` の設定)を済ませてから `cargo test -- --test-threads=1` を実行します。CI(`.github/workflows/test-linux.yml` など)も同じスクリプトを使います。

```terminal
scripts/test.sh                              # すべてのテスト
scripts/test.sh -p juiz_core                 # cargo test に渡す引数
scripts/test.sh -p juiz_core -- p_apply      # -- 以降はテストバイナリに渡す(テスト名の絞り込みなど)
scripts/test.sh -p juiz_core --test integration -- system_test   # juiz_core の統合テストをモジュール名で絞り込む
scripts/test.sh --prepare-only               # 準備だけ行う
```

juiz_core の統合テストは、リンクの時間を減らすため `juiz_core/tests/integration/` の下にまとめて 1 本のバイナリ(`--test integration`)にしています。新しいテストはこのディレクトリにファイルを置き、`main.rs` に `mod` を足してください。ログは `env_logger::try_init()` ではなく `common::logger::init()` で設定します。

必要なものは Rust、CMake と C++ コンパイラ、Python 3 と pyjuiz の依存パッケージ(`python3 -m pip install pyyaml httpx pillow numpy`)です。足りないものがあれば、スクリプトがその旨と入れ方を表示して止まります。
GHC と cabal は任意で、無ければ Haskell の例のビルドとテスト(`haskell_plugin_test`)を飛ばします(CI では `JUIZ_REQUIRE_HASKELL=1` にして飛ばしません)。
`cargo test` だけを実行すると、例のプラグインを読み込むテスト(`system_test` や `*_broker_api_test` など)は失敗します。そのときのエラーには、見つからなかったプラグインのパスや足りない Python モジュールと、必要な準備が表示されます。

### ベンチマークを実行する

ベンチマークは統合テストの中に `#[ignore]` で置いてあり、ふだんのテストでは走りません。数字が意味を持つよう release で実行し、結果は表(Markdown)で出力されます。

```terminal
# 別プロセスへの画像の受け渡し(720p / 1080p / 4K、HTTP と同じプロセス内の比較、issue #230)
cargo test --release -p juiz_core --test integration -- image_transfer_bench --ignored --nocapture --test-threads=1
# Composite と接続の比較(Python の段は scripts/test.sh と同じ準備が要る、issue #169)
PYTHONPATH=bindings/pyjuiz cargo test --release -p juiz_core --test integration -- composite_bench --ignored --nocapture --test-threads=1
```

`image_transfer_bench` は、テストバイナリ自身を別プロセスの juiz として起動し(HTTP はポート 8050、qmp は 8051、IPC は名前 image_transfer_bench)、そこへ HTTP・IPC・qmp で画像を渡します。プラグインは使わないので、事前の準備は要りません。

### VS Code + Dev Containers で開発する

`.devcontainer/` に、CI と同じ環境(Ubuntu 24.04、Rust stable、CMake、Clang、Python 3 と pyjuiz の依存パッケージ)のコンテナ設定があります。
Docker と VS Code の Dev Containers 拡張機能を入れた状態でリポジトリを開き、コマンドパレットから「Dev Containers: Reopen in Container」を選ぶとコンテナ内で開発できます。
`PYO3_PYTHON` と `PYTHONPATH` は設定済みなので、コンテナ内のターミナルで CI と同じ手順でビルドとテストができます。

```terminal
scripts/test.sh
```

HTTP ブローカーの 8000 番ポートはホストに転送されるので、ホストのブラウザから http://localhost:8000/docs を開けます。

`.vscode/` には、コンテナの内外どちらでも使える VS Code の設定があります。

- `extensions.json`: 推奨拡張機能(rust-analyzer、CodeLLDB など。devcontainer と同じもの)
- `settings.json`: 保存時の clippy チェック、rust-analyzer のビルド先の分離(`target/rust-analyzer`)など
- `tasks.json`: CI と同じ手順のビルド・テスト(「Tasks: Run Build Task」「Tasks: Run Test Task」)
- `launch.json`: CodeLLDB で `juiz` コマンドや juiz_core の単体テスト・統合テストをデバッグ実行する設定

## リリース

`v*` タグ(例: `v0.0.26`)を打つと、次のワークフローがそれぞれ公開します。どれもタグの版と各パッケージの版が一致しないと公開せず、公開済みの版は飛ばすので、途中で失敗したらマージ後のコミットにタグを打ち直せば続きから公開されます。

| ワークフロー | 公開先 | 公開するもの | 認証 |
|---|---|---|---|
| `publish.yml` | crates.io | juiz_types / juiz_idl / juiz_macro / juiz_sdk / juiz_std_types / juiz_webui / juiz_core / juiz_qmp_broker / juiz_app | シークレット `CARGO_REGISTRY_TOKEN` |
| `pypi-publish.yml` | PyPI | pyjuiz、juiz-cli | Trusted Publishing(環境 `pypi`) |
| `sdk-release.yml` | GitHub Releases | libjuiz_sdk とヘッダのアーカイブ `juiz_sdk-<版>-<ターゲット名>.tar.gz`(Linux x86_64・aarch64、macOS arm64・x86_64)と `.zip`(Windows x86_64) | `GITHUB_TOKEN` |
| `hackage-publish.yml` | Hackage | hsjuiz(パッケージとドキュメント) | シークレット `HACKAGE_TOKEN` |

版上げでは次のファイルの版をそろえます。

- `Cargo.toml`(`[workspace.dependencies]`)、`Cargo.lock`、`juiz_types` / `juiz_idl` / `juiz_macro` / `juiz_sdk` / `juiz_std_types` / `juiz_webui` / `juiz_core` / `juiz_app` / `brokers/qmp_broker` の `Cargo.toml`
- `bindings/pyjuiz/setup.py`、`bindings/juiz_py/pyproject.toml`(`version` と `pyjuiz==`)、`bindings/juiz_py/Cargo.toml`、`bindings/juiz_py/Cargo.lock`
- `bindings/hsjuiz/hsjuiz.cabal` の `version`(と `bindings/hsjuiz/CHANGELOG.md`)

libjuiz_sdk のアーカイブは `scripts/package-juiz-sdk.sh` で作り、利用者は `scripts/install-juiz-sdk.sh <版>`(Windows は `scripts/install-juiz-sdk.ps1 <版>`)で取得します。
アーカイブには CMake の `find_package(juiz_sdk)` 用の `lib/cmake/juiz_sdk/juiz_sdkConfig.cmake`(元は `bindings/cppjuiz/cmake/juiz_sdkConfig.cmake`)が入り、
`sdk-release.yml` は `bindings/cppjuiz/tests/find_package` をそのアーカイブでビルドして確かめます。
`scripts/check-hsjuiz-package.sh <展開したアーカイブ>` は、hsjuiz を sdist から(Hackage から入れたときと同じ形で)そのアーカイブだけを使ってビルドできるか確かめます。
juiz は C++ / Haskell のプラグインを開いたとき、プラグインが読み込んだ libjuiz_sdk の `juiz_sdk_build_info` を自分の juiz_sdk と比べ、版が違えばエラー、Rust コンパイラの版だけが違えば警告にします。

Hackage への公開には、Hackage のアカウント(アップロード権限つき)で作った API トークンを、リポジトリのシークレット `HACKAGE_TOKEN` に登録しておきます。
登録していないあいだは、`hackage-publish.yml` は確認(sdist・ドキュメントの作成とビルド)だけ行い、警告を出して公開を飛ばします(ほかのワークフローの公開には影響しません)。

## リポジトリ構成

| ディレクトリ | 内容 |
|---|---|
| `juiz_sdk` | 機能要素を開発するための crate |
| `juiz_macro` | `#[juiz_process]` などのマクロ |
| `juiz_types` | Process の引数・戻り値のデータ型 (Avro のスキーマ、指紋、互換性。juiz_sdk に依存しない) |
| `juiz_idl` | IDL (.jidl) の生成器 `juiz-idl` (.avsc・JSON Schema・Rust のコード、build.rs ヘルパ。`juiz idl` からも呼べる)。`juiz_idl/test_package` は生成したコードのテスト。組み込みの標準の型 `juiz_std` の JIDL は `juiz_idl/std` |
| `juiz_std_types` | 標準の型 `juiz_std` の Rust のクレート (組み込みの JIDL から作る)。`juiz_std_types/test_package` は `extern_path` のテスト |
| `juiz_core` | 機能要素を実体化・管理するコア (CoreBroker, Broker など) |
| `juiz_app` | `juiz` コマンド |
| `brokers` | 追加のブローカー実装 (QUIC の `juiz_qmp_broker` など) |
| `ecs` | 実行コンテキスト (TimerEC, OneShotEC, MainLoopEC) |
| `bindings` | C (`cjuiz`: libjuiz_sdk の C の関数の宣言 `juiz_sdk.h`)、C++ (`cppjuiz`)、Haskell (`hsjuiz`)、Python (`pyjuiz`、pip で入れる `juiz` コマンドの `juiz_py`)、JavaScript 向けバインディング |
| `examples` | Rust / C++ / Python / Haskell のサンプル |
| `juiz_tutorial` | チュートリアル |
| `webui` | ブラウザで系を確認する管理画面 (TypeScript + React)。[webui/README.md](webui/README.md) |
| `docs/spec` | 実装の基準にする仕様書 (下の「仕様書」) |

## 仕様書

複数の Step に分けて実装する機能は、最初に仕様を `docs/spec/` に書き、各 Step の実装はそれに従います。実装で仕様を変えるときは、同じ PR で仕様書も直します。

- [Composition(Composite Process / CompositeContainerProcess)](docs/spec/composition.md)(issue #164)
- [データ型(IDL・データモデル・指紋・互換性)](docs/spec/datatype.md)(issue #163)
- [遅延評価(Capsule::Native)](docs/spec/lazy-evaluation.md)(issue #3)
- [アプリ配布形式(.juizapp)](docs/spec/app-package.md)(issue #282)

## ライセンス

明示的に別の意思表示をしない限り、このリポジトリに提出されたコントリビューションは、Apache-2.0 ライセンスの定義に従い、追加の条件なしに MIT OR Apache-2.0 のデュアルライセンスで提供されるものとします。詳しくは [README のライセンス](README.md#ライセンス) を見てください。
