# juiz の開発に参加する

juiz 本体をソースからビルドし、テストを実行するための情報です。
juiz を使ってロボットやIoT機器のソフトウェアを開発する方法は、[公式サイト](https://openjuiz.github.io/ja/)を見てください。

## 必要環境

- Rust 1.89 以降(依存クレート uuid 1.28 の要件。`rust-version` もこれに合わせています)
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
プラグインとしてビルドした `libqmp_broker.so` などを `plugins.broker_factories.qmp_broker` で読み込んだ場合は、そちらが優先されます。ブローカーのプラグインは Rust ABI で読み込むので、読み込む juiz と同じ rustc・同じ版の juiz_sdk でビルドしてください(食い違うと読み込み時にエラーになります)。juiz_core の feature はそろえなくてかまいません(issue #450 で、feature によってトレイトの並びが変わらないようにしました)。`path` は他のプラグインと同じく設定ファイルのあるディレクトリからの相対で書きます(0.0.27 まではブローカーと EC のプラグインだけカレントディレクトリからの相対でした、issue #284)。
自分で `System` を作るアプリケーションでは、`setup()` の前に `qmp_broker::register()` を呼ぶと組み込みで使えます。

### Zenoh ブローカー

Zenoh ブローカー(`brokers/zenoh_broker`、クレート `juiz_zenoh_broker`、[docs/spec/zenoh-broker.md](docs/spec/zenoh-broker.md))は、Zenoh のビルドが重い(juiz_app の初回ビルドが約 2 倍になる)ので、ワークスペースの `default-members` に入れていません。`juiz` コマンドへの組み込みも juiz_app の feature `zenoh-broker`(既定でオフ)です。ふだんの `cargo build` / `cargo test`、`scripts/test.sh`、CI の rust ジョブとカバレッジではビルドしません。

触ったときは手元で `scripts/test-zenoh.sh` を実行します。feature `zenoh-broker` 付きの `juiz` と例のプラグイン、juiz_core の feature なしの Zenoh のプラグインを作ってから、`juiz_zenoh_broker` のテストと juiz_app の `zenoh_cli_test` を走らせます(テストはポート 5212〜5251 を使います)。CI では `test-linux.yml` の `zenoh` ジョブが、別のキャッシュで同じスクリプトを実行します。

```terminal
scripts/test-zenoh.sh
```

`juiz_zenoh_broker` と `juiz_qmp_broker` はどちらもプラグインの入口(`broker_factory` など)を持つので、juiz_app は Zenoh を `default-features = false`(feature `plugin` なし)で組み込みます。両方のクレートを `-p` で選んだうえで juiz_app の `zenoh-broker` を有効にしてビルドすると、入口がリンクでぶつかります。

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

Haskell のサンプル(`examples/haskell`)は cabal でビルドします。GHC 9.6 以降と cabal が必要です(Linux / macOS / Windows)。
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

## テスト

テストの置き場所・名前・実行のしかたの決まりです(issue #518 の整理で決めたもの)。

いまのテストはまだこの決まりにそろっていません。置き場所と名前は issue #521〜#525 で順に移します(juiz_core の integration は #521 で済み)(下の「移行中の扱い」)。新しく書くテストの関数名は今から決まりに合わせてください。

### テストを実行する

`scripts/test.sh` が、テストの前提になる準備(ワークスペースの `cargo build`、C++ の例の cmake ビルド、Haskell の例の cabal ビルド、Python の例が使うパッケージの確認と `PYTHONPATH=bindings/pyjuiz` の設定)を済ませてから `cargo test -- --test-threads=1` を実行します。CI(`.github/workflows/test-linux.yml` など)も同じスクリプトを使います。

```terminal
scripts/test.sh                              # すべてのテスト
scripts/test.sh -p juiz_core                 # cargo test に渡す引数
scripts/test.sh -p juiz_core -- p_apply      # -- 以降はテストバイナリに渡す(テスト名の絞り込みなど)
scripts/test.sh -p juiz_core --test integration -- core::system   # juiz_core の統合テストをモジュール名で絞り込む
scripts/test.sh --prepare-only               # 準備だけ行う
```

必要なものは Rust、CMake と C++ コンパイラ、Python 3 と pyjuiz の依存パッケージ(`python3 -m pip install pyyaml httpx pillow numpy`)です。足りないものがあれば、スクリプトがその旨と入れ方を表示して止まります。
GHC と cabal は任意で、無ければ Haskell の例のビルドとテスト(`plugins::haskell::process`)を飛ばします(CI では `JUIZ_REQUIRE_HASKELL=1` にして飛ばしません)。`juiz pkg create -b haskell` の雛形を作ってパッケージにするテスト(`pkg_cli_test`)は、Linux では patchelf も使い、無ければ飛ばします。
`cargo test` だけを実行すると、例のプラグインを読み込むテスト(`core::system` や `brokers::*_api` など)は失敗します。そのときのエラーには、見つからなかったプラグインのパスや足りない Python モジュールと、必要な準備が表示されます。

テストは `--test-threads=1` で 1 件ずつ走らせます。統合テストや CLI のテストが固定のポート(8000 番台)を使うため、並列にするとポートがぶつかって落ちます。

Rust 以外のテスト:

| 対象 | 置き場所 | 実行 |
|---|---|---|
| pyjuiz | `bindings/pyjuiz/test/*_test.py` | `cd bindings/pyjuiz && python test/<名前>_test.py`。CI は `python.yml` が 1 本ずつ名指しで走らせるので、足したら `python.yml` にも足す(自動で集める形には #525 で変える)。juiz の System が要るものは Rust の統合テスト(`pyjuiz_*`)から起動する |
| 管理画面 | `webui/tests/*.test.ts` | `cd webui && npm test`(`node --test`)。CI は `webui.yml` |

### カテゴリと置き場所

テストは「何を確かめるか」で次のカテゴリに分け、カテゴリごとに置き場所を決めます。迷ったら、そのテストが何を起動するか(何も起動しない / System を起動する / juiz コマンドを起動する / 外のビルドを呼ぶ)で選んでください。

| カテゴリ | 中身 | 置き場所 |
|---|---|---|
| unit | 1 つのモジュールの中だけを確かめる。プラグインもネットワークも使わない | 各クレートの `src/` 内の `#[cfg(test)] mod tests` |
| api | 1 つのクレートの公開 API を外から確かめる。プラグインは使わない | `<クレート>/tests/`。クレートごとに 1 本のバイナリにまとめる(下の「1 クレート 1 バイナリ」) |
| integration | System を起動し、プラグインを読む | `juiz_core/tests/integration/<機能>/`。言語ごとの違いは機能の下に置く。言語の読み込みそのもの(ABI・例外・値の受け渡し)は `plugins/<言語>/` |
| broker | どのブローカーでも同じになるべき確認を、各ブローカーで走らせる | ブローカー共通のスイート `brokers/broker_suite`(下の「ブローカー共通のスイート」)+ ブローカーごとの追加分 |
| cli | `juiz` コマンドを子プロセスで動かす | `juiz_app/tests/cli/` |
| slow | テストの中で外のビルド(cargo・cmake・cabal)を呼ぶ。`pkg_cli_test` など | 置き場所はカテゴリ本来の場所のまま、モジュールパスに `slow` を含めて `-- --skip slow::` で外せるようにする(詳細は #524) |
| bench | 計測。ふだんは走らせない | 置いてある tests の下の `bench/`。`#[ignore]` を付け、release で手動実行する(下の「ベンチマークを実行する」) |

integration の分け方は**機能が先、言語は下**です。何が壊れたかを機能で探すことが多いためです。

```text
juiz_core/tests/integration/
  main.rs                 # mod を並べるだけ
  common/                 # 共通の部品(logger、System の起動など)
  transformations/
    mod.rs
    core.rs               # 言語によらない部分
    rust.rs  python.rs  cpp.rs  haskell.rs
    cross_system.rs
  composition/ ...
  plugins/                # 言語の読み込みそのもの
    cpp/  haskell/  python/  rust/
  bench/
    image_transfer.rs  composite.rs ...
```

1 クレート 1 バイナリ: `tests/` 直下のファイルは 1 本ずつ別のテストバイナリになり、それぞれがクレートと依存を丸ごとリンクします(juiz_app や qmp では 1 本 100MB を超える)。テストは `tests/<まとまり>/main.rs`(juiz_core なら `tests/integration/main.rs`)から `mod` で読み、`tests/` 直下にはファイルを足さないでください。テストで使う conf やプラグインのソース、golden ファイルは `tests/` の下の別ディレクトリ(`confs/`、`python/`、`golden/` など)に置きます。

### ブローカー共通のスイート

どのブローカーでも同じになるべき確認は、公開しないクレート `brokers/broker_suite`(`juiz_broker_suite`)に置き、各ブローカーのテストから呼びます。juiz_core・juiz_qmp_broker・juiz_zenoh_broker は版を書かない path の dev-dependency で使います(crates.io に公開するときには外されます)。

| モジュール | 中身 | 走らせるブローカー |
|---|---|---|
| `api` | BrokerProxy の API を一通り呼ぶ確認(`check_all` と個々の `check_*`)。相手の System の中身は `Fixture` で渡す | core・local・HTTP・IPC(`juiz_core/tests/integration/brokers/*_api.rs`)、qmp・Zenoh |
| `data` | 画像・数の配列・Topic の push をフレームで送る確認と、そのための Process | IPC・qmp・Zenoh |
| `remote` | 別の System のブローカーに繋ぐテストの本体(API・組み込み・ネームスペース・Composite・型・Transformations・モデル)。ブローカーの違いは `RemoteBroker`(型名・プラグイン名・ポートのずらし・組み込みの登録)で渡す | qmp・Zenoh(各 `tests/*_test.rs` は `#[test]` から呼ぶだけ) |
| `fixture` / `support` | Rust の例だけを読んだ System の設定(`RUST_ONLY`)と、小さな部品(`int_of`・`find_process` など) | |

- ブローカーを足すときは、`api::check_all` と(フレームで送るなら)`data` の確認を走らせるテストを書き、そのブローカーだけの確認(HTTP の自分宛ての識別子、qmp の切断、Zenoh の共有メモリ、IPC のソケットの片付けなど)はそのブローカーのテストに置きます。
- ブローカーによって応答の形が違うもの(EC の値が `__value__` で包まれるかどうかなど)は、スイートの側で両方を受け付けます(`support::unwrap_value`)。違いをそろえるのは別の issue で扱います。
- 確認を足すときは、すべてのブローカーで通ることを確かめます(Zenoh は `scripts/test-zenoh.sh`、CI では `test-linux.yml` の zenoh ジョブ)。

### 名前

- **ファイル名・モジュール名**は機能名だけにします。`_test` は付けません(ディレクトリがテストであることと分類を表します)。例: `transformations/cross_system.rs`。
- ただし `literals`・`names`・`errors` のように、それだけでは何のことか分からない名前は避けます(`idl_literals`、`namespace_names` などにする)。
- **テスト関数名**は「何をすると・どうなる」を英語の文で書きます。`test_` 接頭辞、`_test` 接尾辞は付けません。
  - 良い例: `call_with_omitted_argument_uses_default`、`connect_rejects_mismatched_types`、`remove_subsystem_drops_topic_forwarding`
  - 避ける例: `test_call`、`call_test`、`test1`
- 関数名がモジュールパスと合わさって読めれば十分です。`transformations::cross_system::lookup_follows_remote_parent` のように、モジュールで言えていることを関数名で繰り返す必要はありません。
- **補助関数**(`#[test]` の付かない関数)はテスト関数と見分けやすい動詞で始めます(`start_system`、`check_profile`、`make_conf` など)。複数のファイルで使うものは `common/` に置きます。
- Python と TypeScript のテストは、それぞれの道具が集める名前(`*_test.py`、`*.test.ts`)に従います。中の関数名は Rust と同じく「何がどうなる」で書きます。

### 絞り込み

テストはモジュールパスで絞り込みます。ディレクトリを機能で分けているので、機能や言語の単位で走らせられます。

```terminal
scripts/test.sh -p juiz_core --test integration -- transformations::          # Transformations の統合テストすべて
scripts/test.sh -p juiz_core --test integration -- transformations::python    # その Python の分だけ
scripts/test.sh -p juiz_core --test integration -- plugins::haskell::         # Haskell の読み込み
scripts/test.sh -p juiz_app --test cli -- --skip slow::                       # 遅いものを外す(#523・#524 の後)
```

### テストを書くときの決まり

- **logger**: 1 本のバイナリで全テストが走るので、`env_logger::try_init()` ではなく `common::logger::init()` を使います。
- **ポート**: 固定のポートを使うときは、ほかのテストが使っていない番号を選びます(`grep -rn "80[0-9][0-9]" */tests brokers/*/tests` などで確かめる)。同じポートを使うテストが同じバイナリにあると、前のテストが閉じ切る前に次が始まって落ちることがあります。
- **前提が無いとき**: 任意の道具(GHC・cabal・patchelf など)が無いときは、テストの中で理由を表示して早めに戻ります(`scripts/test.sh` が CI で必須にしているものは除く)。黙って通さず、何を飛ばしたかを出力に残してください。
- **既定でスキップしない**: 失敗するからといって `#[ignore]` を付けたりテストを消したりしません。`#[ignore]` は bench と、手で条件を用意するテスト(理由を `#[ignore = "..."]` に書く)だけに使います。
- **何を確かめるか**: 1 つのテスト関数では 1 つの振る舞いを確かめます。同じ準備で複数の振る舞いを確かめたいときは、準備を補助関数にして関数を分けます。

### 消す・まとめるとき

重なっているテストを消すときは次に従います。

- 同じ経路を同じ入力で確かめているものは、新しい方を残します。
- ブローカーの違いだけのものはブローカー共通のスイートに寄せ、ブローカー固有の確認(qmp の切断、Zenoh の共有メモリなど)だけ個別に残します。
- 消すテストは、PR の本文に「どのテストに含まれるか」を 1 件ずつ書きます。
- カバレッジ(行)が下がらないことを確かめます(`cargo llvm-cov`、CI では `coverage.yml` を手動で実行できます)。

### 移行中の扱い

置き場所と名前は次の順にそろえます。それぞれが終わるまでは、そのテストは今の場所に足してかまいません(関数名だけは上の決まりに合わせる)。

| issue | 内容 |
|---|---|
| #521 | `juiz_core/tests/integration` を機能ごとのディレクトリに分け、名前を決まりにそろえる。ベンチを `bench/` へ(済み) |
| #522 | ブローカー共通のスイート(juiz_core の `brokers::{core,local,http,ipc}_api`、qmp・Zenoh の `*_broker_api_test` と、ほぼ同じ写しの qmp・Zenoh の tests をまとめる)(済み) |
| #523 | `juiz_app`・qmp・Zenoh・`juiz_sdk` のテストをクレートごとに 1 本のバイナリへ |
| #524 | slow の印と、Python 3.10 のジョブで走らせるテストの絞り込み |
| #525 | pyjuiz のテストを自動で集める形に |

### ベンチマークを実行する

ベンチマークは統合テストの中に `#[ignore]` で置いてあり、ふだんのテストでは走りません。数字が意味を持つよう release で実行し、結果は表(Markdown)で出力されます。

```terminal
# 別プロセスへの画像の受け渡し(720p / 1080p / 4K、HTTP と同じプロセス内の比較、issue #230)
cargo test --release -p juiz_core --test integration -- bench::image_transfer --ignored --nocapture --test-threads=1
# Composite と接続の比較(Python の段は scripts/test.sh と同じ準備が要る、issue #169)
PYTHONPATH=bindings/pyjuiz cargo test --release -p juiz_core --test integration -- bench::composite --ignored --nocapture --test-threads=1
```

Zenoh ブローカーのベンチは juiz_core に置かず(Zenoh を juiz_core のテストでビルドしないため)、`juiz_zenoh_broker` のテストにあります。

```terminal
cargo test --release -p juiz_zenoh_broker --test zenoh_bench_test -- --ignored --nocapture --test-threads=1
```

`bench::image_transfer` は、テストバイナリ自身を別プロセスの juiz として起動し(HTTP はポート 8050、qmp は 8051、IPC は名前 image_transfer_bench)、そこへ HTTP・IPC・qmp で画像を渡します。プラグインは使わないので、事前の準備は要りません。

## リリース

`v*` タグ(例: `v0.0.26`)を打つと、次のワークフローがそれぞれ公開します。どれもタグの版と各パッケージの版が一致しないと公開せず、公開済みの版は飛ばすので、途中で失敗したらマージ後のコミットにタグを打ち直せば続きから公開されます。

| ワークフロー | 公開先 | 公開するもの | 認証 |
|---|---|---|---|
| `publish.yml` | crates.io | juiz_types / juiz_idl / juiz_pkg / juiz_macro / juiz_sdk / juiz_std_types / juiz_webui / juiz_core / juiz_qmp_broker / juiz_app | シークレット `CARGO_REGISTRY_TOKEN` |
| `pypi-publish.yml` | PyPI | pyjuiz、juiz-cli | Trusted Publishing(環境 `pypi`) |
| `sdk-release.yml` | GitHub Releases | libjuiz_sdk とヘッダのアーカイブ `juiz_sdk-<版>-<ターゲット名>.tar.gz`(Linux x86_64・aarch64、macOS arm64・x86_64)と `.zip`(Windows x86_64) | `GITHUB_TOKEN` |
| `hackage-publish.yml` | Hackage | hsjuiz(パッケージとドキュメント) | シークレット `HACKAGE_TOKEN` |

版上げでは次のファイルの版をそろえます。

- `Cargo.toml`(`[workspace.dependencies]`)、`Cargo.lock`、`juiz_types` / `juiz_idl` / `juiz_pkg` / `juiz_macro` / `juiz_sdk` / `juiz_std_types` / `juiz_webui` / `juiz_core` / `juiz_app` / `brokers/qmp_broker` の `Cargo.toml`
- `bindings/pyjuiz/setup.py`、`bindings/juiz_py/pyproject.toml`(`version` と `pyjuiz==`)、`bindings/juiz_py/Cargo.toml`、`bindings/juiz_py/Cargo.lock`
- `bindings/hsjuiz/hsjuiz.cabal` の `version`(と `bindings/hsjuiz/CHANGELOG.md`)

libjuiz_sdk のアーカイブは `scripts/package-juiz-sdk.sh` で作り、利用者は `scripts/install-juiz-sdk.sh <版>`(Windows は `scripts/install-juiz-sdk.ps1 <版>`)で取得します。
アーカイブには CMake の `find_package(juiz_sdk)` 用の `lib/cmake/juiz_sdk/juiz_sdkConfig.cmake`(元は `bindings/cppjuiz/cmake/juiz_sdkConfig.cmake`)が入り、
`sdk-release.yml` は `bindings/cppjuiz/tests/find_package` をそのアーカイブでビルドして確かめます。
`scripts/check-hsjuiz-package.sh <展開したアーカイブ>` は、hsjuiz を sdist から(Hackage から入れたときと同じ形で)そのアーカイブだけを使ってビルドできるか確かめます。
juiz は C++ / Haskell のプラグインを開いたとき、プラグインが読み込んだ libjuiz_sdk の `juiz_sdk_build_info` を自分の juiz_sdk と比べ、版が違えばエラー、Rust コンパイラの版だけが違えば警告にします。

Hackage への公開には、Hackage のアカウント(アップロード権限つき)で作った API トークンを、リポジトリのシークレット `HACKAGE_TOKEN` に登録しておきます。
登録していないあいだは、`hackage-publish.yml` は確認(sdist・ドキュメントの作成とビルド)だけ行い、警告を出して公開を飛ばします(ほかのワークフローの公開には影響しません)。

### 保守用の枝(release/0.1)

0.1 系は保守用の枝 `release/0.1` で直し、機能追加は `main`(次の 0.2 系)で進めます。保守する枝はいつも 1 本だけにし、0.2.0 を出したら `release/0.1` の保守は止めます。

- `release/0.1` には不具合の修正だけを入れます。機能追加や API の変更は入れません。
- 修正はまず `main` への PR で入れ、マージ後にそのコミットを `release/0.1` へ cherry-pick します(`git cherry-pick -x <コミット>` で元のコミットを記録し、`release/0.1` を向いた PR にします)。`main` ですでに形が変わっていて同じ直し方ができないときは、`release/0.1` 向けに別に直します。
- `release/0.1` を `main` にまとめてマージすることはしません(修正は `main` に先に入っているため)。
- パッチ版(0.1.1、0.1.2 …)を出すときは、`release/0.1` の上で上の「版上げでそろえるファイル」を版上げする PR を作ってマージし、そのマージ後のコミットに `v0.1.x` のタグを打ちます。リリース作成画面から打つときは Target に `release/0.1` を選びます。
- 公開のワークフローはタグを打ったコミットにあるものが動くので、`release/0.1` の上のワークフローも公開できる状態に保ちます(ワークフローの修正も cherry-pick します)。
- crates.io・PyPI・Hackage には古い系列の版も後から出せます。GitHub Releases は、v*.*.* のタグの中で一番新しい版のときだけ「Latest」になります(`sdk-release.yml`)。
- テスト(`test-linux.yml`・`test-macos.yml`・`test-windows.yml`・`python.yml`・`webui.yml`)は `main` と同じく `release/**` への push でも動きます。

## リポジトリ構成

| ディレクトリ | 内容 |
|---|---|
| `juiz_sdk` | 機能要素を開発するための crate |
| `juiz_macro` | `#[juiz_process]` などのマクロ |
| `juiz_types` | Process の引数・戻り値のデータ型 (Avro のスキーマ、指紋、互換性。juiz_sdk に依存しない) |
| `juiz_idl` | IDL (.jidl) の生成器 `juiz-idl` (.avsc・JSON Schema・Rust のコード、build.rs ヘルパ。`juiz idl` からも呼べる)。`juiz_idl/test_package` は生成したコードのテスト。組み込みの標準の型 `juiz_std` の JIDL は `juiz_idl/std` |
| `juiz_pkg` | 雛形を作る `juiz-pkg`(`juiz pkg create` / `juiz pkg add` / `juiz pkg target` からも呼べる、[docs/spec/pkg.md](docs/spec/pkg.md))。テンプレートは `juiz_pkg/templates/`(build.rs でバイナリに埋め込む) |
| `juiz_std_types` | 標準の型 `juiz_std` の Rust のクレート (組み込みの JIDL から作る)。`juiz_std_types/test_package` は `extern_path` のテスト |
| `juiz_core` | 機能要素を実体化・管理するコア (CoreBroker, Broker など) |
| `juiz_app` | `juiz` コマンド |
| `brokers` | 追加のブローカー実装 (QUIC の `juiz_qmp_broker`、Zenoh の `juiz_zenoh_broker`) |
| `ecs` | 実行コンテキスト (TimerEC, OneShotEC, MainLoopEC) |
| `bindings` | C (`cjuiz`: libjuiz_sdk の C の関数の宣言 `juiz_sdk.h`)、C++ (`cppjuiz`)、Haskell (`hsjuiz`)、Python (`pyjuiz`、pip で入れる `juiz` コマンドの `juiz_py`)、JavaScript 向けバインディング |
| `examples` | Rust / C++ / Python / Haskell のサンプル |
| `webui` | ブラウザで系を確認する管理画面 (TypeScript + React)。[webui/README.md](webui/README.md) |
| `docs/spec` | 実装の基準にする仕様書 (下の「仕様書」) |

## 仕様書

複数の Step に分けて実装する機能は、最初に仕様を `docs/spec/` に書き、各 Step の実装はそれに従います。実装で仕様を変えるときは、同じ PR で仕様書も直します。

- [Composition(Composite Process / CompositeContainerProcess)](docs/spec/composition.md)(issue #164)
- [データ型(IDL・データモデル・指紋・互換性)](docs/spec/datatype.md)(issue #163)
- [遅延評価(Capsule::Native)](docs/spec/lazy-evaluation.md)(issue #3)
- [アプリ配布形式(.juizapp)](docs/spec/app-package.md)(issue #282)
- [Rust プラグインの C ABI](docs/spec/rust-plugin-abi.md)(issue #291)
- [雛形生成コマンド(juiz pkg)](docs/spec/pkg.md)(issue #455)
- [Transformations(Container の位置姿勢と座標変換)](docs/spec/transformations.md)(issue #366)
- [Container の 3D モデル(ContainerModel)](docs/spec/container-model.md)(issue #379)
- [Zenoh ブローカー](docs/spec/zenoh-broker.md)(issue #487)
- [Manifest と Profile の用語](docs/spec/manifest-profile.md)(issue #503)

## ライセンス

明示的に別の意思表示をしない限り、このリポジトリに提出されたコントリビューションは、Apache-2.0 ライセンスの定義に従い、追加の条件なしに MIT OR Apache-2.0 のデュアルライセンスで提供されるものとします。詳しくは [README のライセンス](README.md#ライセンス) を見てください。
