# Zenoh ブローカー仕様

- 状態: 確定(Zenoh ブローカー Step 0、issue #489、2026-11-14)。§11 は Step 3(#492、2026-11-14)で決めた
- 親 issue: #487。計画と経緯はプロジェクトの計画書 zenoh-broker-plan.md(2026-11-14)にある
- 関連: フレーム形式は [large-data.md](large-data.md) §3、qmp の決めごとは同 §4、プラグインの境界は [rust-plugin-abi.md](rust-plugin-abi.md) §12(#450)、似た作りの [ipc-broker.md](ipc-broker.md)
- この文書は Step 1〜5(#490〜#493、site#96)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と範囲

別プロセス・別ホストの juiz を、qmp(QUIC)より速く呼び出すためのブローカー。種類名は `zenoh`。

- 通り道は [Zenoh](https://zenoh.io/) 1.x の TCP。同じホストの相手へは、大きなフレームを Zenoh の共有メモリで送る(§11、Step 3)。
- 要求と応答は large-data.md §3 のフレーム(先頭 `JZF1`)。画像は PNG にせず生バイトで送る。
- call も Topic の push も、Zenoh の query / reply に載せる(§5)。Topic を Zenoh の put / subscribe にするかは Step 1 の数字を見て別に決める(§12)。
- juiz_app には既定オフの feature `zenoh-broker` で入れる。プラグインとしても読める(§8)。

juiz の外で測った値(クラウド 4 vCPU、別プロセス、送るだけ、1 回あたり):

| 方式 | 720p | 4K |
|---|---|---|
| Zenoh TCP | 1.2 ms | 9.6 ms |
| Zenoh 共有メモリ(毎回写す) | 0.89 ms | 3.0 ms |
| 参考: qmp(juiz の中、large-data.md §4) | 約 11 ms | – |

## 2. クレートと依存

- `brokers/zenoh_broker`。パッケージ名 `juiz_zenoh_broker`、ライブラリ名 `zenoh_broker`(プラグインのファイル名 `libzenoh_broker.so` などと conf の `plugins.broker_factories.zenoh_broker` のため)、`crate-type = ["cdylib", "rlib"]`。qmp と同じ形にする。
- 依存は `zenoh = { version = "1.10.2", default-features = false, features = ["transport_tcp"] }`。ほかの transport(UDP・QUIC・WebSocket など)は入れない。共有メモリは juiz_zenoh_broker の feature `shared-memory`(既定オン)で zenoh の `shared-memory` と `unstable` を足す(§11.1)。
- Zenoh のライセンスは EPL-2.0 OR Apache-2.0。juiz からは Apache-2.0 として使う。
- バージョンは他のクレートと同じ(ワークスペースの版)。crates.io への公開は Step 2 で publish.yml に足す。

### 2.1 ワークスペースと CI 時間

Zenoh を足すと juiz_app の初回ビルドが 86 秒から 190 秒になる(4 コア debug、依存 419 → 576 クレート)。既存の CI の時間を変えないため、次を守らなければならない。

- `brokers/zenoh_broker` はワークスペースの `members` に入れ、`default-members` には入れない。`scripts/test.sh` の `cargo build` / `cargo test` と、カバレッジのジョブ(`default-members` からパッケージを選ぶ)は Zenoh をビルドしない。
- juiz_app の feature `zenoh-broker` は `default` に入れない(§8.2)。
- juiz_core に `juiz_zenoh_broker` を dev-dependency として足さない(qmp は `image_transfer_bench` のために足しているが、そうすると juiz_core のテストが Zenoh をビルドする)。Zenoh のテストとベンチは `brokers/zenoh_broker/tests` に置く(§10)。
- CI は Zenoh だけの別ジョブ(`test-linux.yml` の `zenoh`、キャッシュも別)にする。ジョブも手元も `scripts/test-zenoh.sh` で、feature `zenoh-broker` 付きの `juiz`・例のプラグイン・juiz_core の feature なしの Zenoh のプラグインを作り、`cargo test -p juiz_zenoh_broker` と juiz_app の `zenoh_cli_test` を走らせる(#491)。

## 3. ID と名前

- ブローカーの ID は `zenoh://<host>:<port>`。オブジェクトの ID は http・qmp と同じく `zenoh://<host>:<port>/process/<ns>/<name>::<type>` の形になる。
- ブローカーの名前(`name`)は `<host>:<port>` で、qmp と同じ規則にする。起動したブローカーは実際に listen したアドレスで名前を付け直す(`0.0.0.0` は `127.0.0.1` に、ポート 0 は割り当てられた番号に)。
- BrokerProxy の名前 `<host>:<port>` から、Zenoh の locator `tcp/<host>:<port>` を作って繋ぐ。IPv6 は `[<addr>]:<port>` と書き、locator も `tcp/[<addr>]:<port>` にする。
- ID に Zenoh の locator(`tcp/...`)をそのまま入れない。juiz の ID の `/` 区切りと混ざるため。TCP 以外の locator で繋ぎたいときは conf の `zenoh_config`(§4.3)で足す。
- サーバーは qmp と同じく、要求の `identifier` が自分を指す `zenoh://<自分の名前>/...` なら `core://core/...` に書き換え、`param` に `accessed_broker_id`(`zenoh://<name>`)・`accessed_broker_type`(`zenoh`)・`accessed_broker_name` を入れる(`localize_identifier`)。
- Zenoh の query からは相手の IP アドレスが取れないので、`requesting_host_addr` は付けない(IPC と同じく `None`)。

## 4. conf

### 4.1 ブローカー(サーバー)

```yaml
brokers:
  - type_name: zenoh
    host: 0.0.0.0               # 省略すると 0.0.0.0
    port: 7448                  # 省略すると 7448
    max_frame_bytes: 1073741824 # 1 フレームの上限 (large-data.md §3.5)。省略してよい
    shared_memory: true         # 同じホストの相手へ共有メモリで送るか (§11.3)。省略すると true
    shm_pool_bytes: 67108864    # 共有メモリの pool の大きさ (§11.3)。省略すると 64 MiB
    zenoh_config: {}            # Zenoh の設定に重ねるもの (§4.3)。省略してよい
```

- 既定のポートは 7448 にする。Zenoh の既定 7447 は zenohd(Zenoh のルーター)が使うので、同じホストで zenohd を動かしても重ならないようにする。
- listen する locator は `tcp/<host>:<port>` の 1 つ。

### 4.2 ブローカープロキシ(クライアント)

```yaml
broker_proxies:
  - type_name: zenoh
    name: 127.0.0.1:7448        # 繋ぎ先 (§3)
    timeout_ms: 10000           # 応答の待ち時間の上限。省略すると 10 秒
    max_frame_bytes: 1073741824 # 省略してよい
    shared_memory: true         # 省略してよい (§11.3)
    shm_pool_bytes: 67108864    # 省略してよい (§11.3)
    zenoh_config: {}            # 省略してよい
```

- `subsystems:` / `mastersystem:` の繋ぎ先も、http・qmp と同じく `zenoh://<host>:<port>` で書ける。
- `juiz -s zenoh://<host>:<port> process list` のように、CLI の `-s` に ID を渡せる。サーバーを起動する CLI のオプション(`--ipc` にあたるもの)は作らない。conf で起動する。

### 4.3 Zenoh の設定

juiz が組み立てる Zenoh の設定は次のとおり。

| キー | サーバー | クライアント | 理由 |
|---|---|---|---|
| `mode` | `"peer"` | `"client"` | クライアントは繋いだ 1 つの相手とだけ話す(§5.1) |
| `listen/endpoints` | `["tcp/<host>:<port>"]` | 設定しない | |
| `connect/endpoints` | 設定しない | `["tcp/<host>:<port>"]` | |
| `connect/timeout_ms` | – | `5000` | qmp の接続待ちと同じ 5 秒。繋がらなければ open がエラーになる(`exit_on_failure` は client の既定の true のまま) |
| `scouting/multicast/enabled` | `false` | `false` | 同じネットワークの他の Zenoh を勝手に見つけて繋がないため |
| `scouting/gossip/enabled` | `false` | `false` | 同上 |
| `transport/link/rx/max_message_size` | `max_frame_bytes` | `max_frame_bytes` | これを超えるメッセージを Zenoh は黙って捨てるので、フレームの上限とそろえる(§6) |
| `transport/shared_memory/transport_optimization/pool_size` | `shm_pool_bytes` | `shm_pool_bytes` | 共有メモリの pool(§11.3)。feature `shared-memory` があり `shared_memory` が true のときだけ |
| `transport/shared_memory/enabled` | `shared_memory` が false なら `false` | 同左 | feature `shared-memory` があるときだけ。true のときは Zenoh の既定(true)のまま |

- conf の `zenoh_config` は JSON5 のキーと値の組(例 `{"transport/link/tx/batch_size": 65535}`)で、上の表を組み立てた後に `insert_json5` で 1 つずつ重ねる。表のキーも上書きできるが、§5 の決めごと(1 つの相手とだけ話す)を崩したときの振る舞いは保証しない。
- 設定に誤りがあれば、ブローカーは起動時に、プロキシは作るときにエラーにする。

## 5. 要求と応答

### 5.1 key expression

- サーバーは、ブローカー 1 つにつき queryable を 1 つ、key expression `juiz/v1/crud` に宣言する。すべての要求(call・Topic の push・list・create など)はこの 1 つで受ける。どの処理かはフレームのヘッダの `param`(`method_name`・`class_name`・`function_name`)で決まる。
- key expression に System の名前やネームスペースを入れない。ネームスペースは juiz の ID(`identifier`)の中にあり、振り分けは IPC・qmp と同じく juiz_core がする。
- `v1` は要求の形の版。フレームの形を互換なく変えるときは `v2` にし、古いものとは繋がらないようにする(qmp の新旧のように、届いてから先頭の 4 バイトで失敗するより先に分かる)。
- 固定の key で誤配しないのは、クライアントが `mode: client` で 1 つのサーバーにだけ繋ぎ、サーバー同士が scouting で繋がらないから。BrokerProxy ごとに Zenoh のセッションを 1 つ開く(同じ相手への BrokerProxy は今と同じく 1 つ)。
- Topic を put / subscribe にするときは `juiz/v1/topic/<ns>/<topic 名>` を使う(予約。§12)。

### 5.2 クライアント(`ZenohBrokerProxy`)

- qmp の BrokerProxy と同じく、要求ごとに `param` と `map` を組み立てる専用の BrokerProxy にする(`MessengerBrokerProxy` は使わない。`method_name` は小文字)。
- 要求のフレームを `Vec<u8>` に書き、`ZBytes::from(Vec<u8>)`(写しなし)にして `session.get("juiz/v1/crud").payload(..)` で送る。`target` は `BestMatching`、`consolidation` は `None`、`timeout` は `timeout_ms`。
- 最初に届いた reply を応答とする。reply の payload を `ZBytes::reader()`(`std::io::Read`)で `juiz_core::frame::read_frame` に渡し、まとめて 1 つの領域に写さずに読む(Zenoh が分割して受け取った断片から、本体を最終的な領域へ直接読む)。
- 応答の `id` が要求と合わなければエラーにする。
- フレームが `max_frame_bytes` を超えるときは、送る前にエラーにする(Zenoh に渡すと黙って捨てられ、待ち時間切れでしか分からないため)。
- セッションは BrokerProxy を作るときに開く。開けなければ(繋がらない)作るのを失敗にする。Zenoh は切れたら自分で繋ぎ直すので、セッションは作り直さない。
- 要求は送り直さない。query を送った後に失敗したかどうかは、サーバーで処理が済んだかどうか分からないため(IPC と同じ)。
- 同期の `.wait()` で待つ。tokio の非同期タスクの中からは呼ばない(qmp の `futures::executor::block_on` と同じ制約)。HTTP の処理から Zenoh の BrokerProxy を呼ぶ場合(副系へ転送するとき)を Step 1 のテストで確かめる。

### 5.3 サーバー

- `on_start` で Zenoh のセッションを開き、§5.1 の queryable を宣言する。受け取りは Zenoh のチャンネル(FIFO のハンドラ)にし、受け取り用の普通のスレッドで `recv()` する。Zenoh の受け取りのコールバックの中で処理しない(Zenoh の内部のスレッドを止めるため)。
- query を 1 つ受け取るたびに、処理用の普通のスレッドを作る(tokio のタスクにしない。#231・IPC と同じ理由)。query は並行に処理する(HTTP の #356 と同じく、同じ Process への呼び出しも並行に走る)。スレッドの数の上限は設けない。数が問題になれば Step 1 でスレッドプールにしてよい。
- payload を §5.2 と同じく `reader()` で読み、`CRUDBroker` の振り分け(`on_capsule_map_request`)に `CapsuleMap` を直接渡す。応答のフレームを `Vec<u8>` に書き、`query.reply("juiz/v1/crud", ..)` で返す。
- 処理が失敗したら、必ずエラー応答のフレーム(large-data.md §3.2 の `kind: "error"`)を `reply` で返す。Zenoh の `reply_err` は使わない(エラーの形を IPC・qmp とそろえるため)。payload が無い・フレームが読めないときも、読めた範囲の `id`(無ければ 0)でエラー応答を返す。
- `listen` できなければ(ポートが使用中など)、ログに ERROR を出して起動を失敗にする。
- 止めるときは queryable を取り消し、セッションを閉じる。

## 6. エラーと待ち時間

| 起きたこと | クライアントが返すもの |
|---|---|
| サーバーで処理が失敗した(エラー応答のフレーム) | `BrokerProxyRemoteError`(IPC・qmp と同じ。`class_name`・`function_name` を持つ) |
| 繋がらない(BrokerProxy を作るとき) | エラー。`zenoh://<name>: can not connect (tcp/<host>:<port>)` |
| `timeout_ms` までに reply が来ない | エラー。`zenoh://<name>: no response within <timeout_ms> ms` |
| reply が 1 つも無いまま終わった(相手に queryable が無い) | エラー。`zenoh://<name>: no juiz zenoh broker answered`(繋ぎ先が juiz でない、または `v1` でない) |
| Zenoh の `ReplyError` が届いた | エラー。中身を文字列にして付ける |
| フレームが読めない・`id` が合わない | エラー |
| 送るフレームが `max_frame_bytes` を超える | 送らずにエラー |

- 待ち時間の既定は 10 秒(Zenoh の `queries_default_timeout` と IPC の既定と同じ)。それより長くかかる Process を呼ぶときは `timeout_ms` を伸ばす。
- サーバーの `max_frame_bytes` を超える要求は、Zenoh が受け取り側で捨てるので、クライアントでは待ち時間切れになる。どちらも既定の 1 GiB を使っていれば起きない。

## 7. Topic

- Topic の push は、qmp と同じく BrokerProxy の要求(§5.2)として送る。送り手の System が受け手の System の Zenoh ブローカーに query を送り、受け手が reply を返す。
- Zenoh の put / subscribe は、Step 1 では使わない(§12)。

## 8. 組み込みとプラグイン

### 8.1 クレートの入口

qmp と同じものを置く。

- `zenoh_broker_factory` / `zenoh_broker_proxy_factory`(`type_name: "zenoh"`)。
- `register()`: `juiz_core::register_builtin_broker_factory` で組み込みとして登録する。自分で `System` を作るアプリは `setup()` の前に呼ぶ。
- プラグインとしての入口 `broker_factory`・`broker_proxy_factory`(`extern "Rust"`)と、`broker_factory_build_info`(`extern "C"`、`juiz_sdk::build_info::build_info_ptr()` を返す)。rust-plugin-abi.md §12 のとおり、juiz_core は `broker_factory` の前にこれで rustc と juiz_sdk の版を比べる。
- プラグインとの境界(`BrokerProxy` などのトレイト)に `cfg(feature)` のメソッドや欄を足さない(#450)。Step 3 の共有メモリの feature も、境界の型を変えない形で入れる。
- プラグインとしての入口(`#[no_mangle]` の 3 つ)は feature `plugin`(既定オン)のときだけ置く。qmp_broker も同じ名前の入口を持つので、両方を rlib として 1 つの実行ファイルに入れるとリンクで名前がぶつかる。juiz_app は `default-features = false` で組み込む(#491)。

### 8.2 juiz_app

- feature `zenoh-broker = ["dep:juiz_zenoh_broker", "juiz_zenoh_broker/shared-memory"]`(共有メモリも入れる、§11.1)。`default` に入れない。有効なとき、`main_with_args` で `zenoh_broker::register()` を呼ぶ。
- 同じ `type_name` のプラグインを `plugins.broker_factories.zenoh_broker` で読んだときは、qmp と同じくプラグインを優先する。
- feature が無い juiz で conf に `type_name: zenoh` を書いたとき、または `-s zenoh://...` で繋ごうとしたときは、「知らないブローカーの種類」のエラー(`BrokerFactoryCanNotFoundError` / `FactoryCanNotFoundError`)に、`--features zenoh-broker` でビルドするかプラグインを読むよう案内を足す。qmp も同じ(`juiz_core::brokers::builtin_broker_factories::broker_factory_not_found`)。
- juiz_py には同じ名前の feature `zenoh-broker`(`juiz_app/zenoh-broker` を有効にする)を置く。pip の juiz-cli の wheel で使うかは §8.3。

### 8.3 pip の juiz-cli の wheel

juiz_py(PyPI の juiz-cli)の wheel を feature `zenoh-broker` ありとなしでビルドして比べた(クラウドの 4 vCPU、Linux x86_64、`maturin build --release`、キャッシュなし、2026-11-14)。

| | なし | あり | 差 |
|---|---|---|---|
| wheel の大きさ | 14.0 MB | 18.0 MB | +4.0 MB(+29%) |
| 展開後の `_native.abi3.so` | 42.6 MB | 56.3 MB | +13.7 MB |
| release ビルドの時間 | 202 秒 | 297 秒 | +95 秒(+47%) |

- pip の利用者はプラグインを自分でビルドしにくい(ホストと同じ rustc が要る)ので、公開する wheel には Zenoh を組み込む。
- CI の時間を変えないため、`juiz-wheels.yml` は PR では feature なしでビルドし、PR 以外(main・release/* への push、タグでの公開、手動実行)でだけ `--features zenoh-broker` を付ける。そのときはスモークテスト(`bindings/juiz_py/tests/smoke_test.py`、環境変数 `JUIZ_SMOKE_ZENOH=1`)で `zenoh://` 経由の呼び出しも確かめる。
- PR で作る wheel と公開する wheel の構成が違うことになる。Zenoh の部分は `test-linux.yml` の `zenoh` ジョブで、wheel の組み立ては main への push で確かめる。

## 9. 制限

- アクセス制御も暗号化もしない(qmp は QUIC で暗号化されるが、こちらは平文の TCP)。信頼できるネットワークの中で使う。Zenoh の TLS・認証は対象外(§13)。
- サーバーが受けるのは同じ `v1` の要求だけ。
- 同じ相手への BrokerProxy ごとに Zenoh のセッション(と Zenoh の内部のスレッド)ができる。副系が多いときの重さは Step 1 で測る。

## 10. テストとベンチ(Step 1)

- `brokers/zenoh_broker/tests` に、qmp の `qmp_broker_api_test.rs` を雛形に、同じプロセスの中でのブローカー越しの呼び出しと、子プロセスへの呼び出しの両方を置く。ネームスペース・Composite・型・Transformations の API も qmp と同じだけ確かめる。
- 画像(720p の `rgb8`、`l16`・`rgba32f` など形式ごと)の引数と戻り値、数の配列、Topic の push で画像を送ること。
- エラー応答、繋がらないとき、待ち時間切れ、サーバーを再起動した後の要求、`max_frame_bytes` を超えるフレーム。
- 繋ぎ先は `tcp/127.0.0.1:<port>`(クラウドの環境には IPv6 が無い)。ポートは他のテストと重ならない番号を Step 1 で選ぶ。
- feature なしの juiz_core で作ったプラグインを読むテスト(#450 の `broker_plugin_features_cli_test` と同じ形)。
- ベンチは large-data.md の `image_transfer_bench` と同じ大きさ(720p・4K)の call と Topic の push を、Zenoh のクレートの中のテストとして置き、qmp・IPC と並べて数字を残す(§2.1 のとおり juiz_core には置かない)。

### 10.1 Step 1 で置いたもの(#490)

- テストは `brokers/zenoh_broker/tests` の `zenoh_*_test.rs`。qmp のテスト(API・組み込み・Composite・3D モデル・ネームスペース・Transformations・型)を同じ形で置き、`zenoh_errors_test.rs` に上の 3 つ目の項目と、HTTP の処理から Zenoh の BrokerProxy を呼ぶ転送(§5.2)を置いた。ポートは 5212〜5250(HTTP は 8232〜8269)。
- 別プロセスのテストは、`juiz` コマンドに Zenoh のプラグインを読ませる(組み込みは Step 2)。プラグインは `cargo test -p juiz_zenoh_broker` が juiz_core の feature なしで作るので、既定の feature の juiz に feature なしのプラグインを読ませる確かめ(#450)を兼ねる。別のテストは足していない。
- ブローカーの `name` は省いてよい。省くと `<host>:<port>` にし、起動後に実際の住所に付け直す(§3)。
- ベンチは `zenoh_bench_test.rs`(`#[ignore]`)。同じクラウドの環境(4 vCPU、release)で `image_transfer_bench` と並べた値(1 回あたり [ms]):

| 大きさ | 方式 | call の戻り値 | 引数で送る | Topic の push |
|---|---|---|---|---|
| 720p | Zenoh(TCP) | 2.9 | 2.9 | 2.8 |
| 720p | qmp | 7.4 | 8.4 | – |
| 720p | IPC | 1.5 | 2.3 | 1.7(A、届くまで) |
| 4K | Zenoh(TCP) | 25〜27 | 25〜27 | 26〜28 |
| 4K | qmp | 73.6 | 81.4 | – |
| 4K | IPC | 10.6 | 88.6 | 78.2(A、届くまで) |

Zenoh は qmp より 720p で約 2.5 倍、4K で約 3 倍速い。Zenoh の Topic の push は受け手の Process の実行が終わって応答が返るまでの時間。juiz の外で測った値(§1、720p 1.2 ms)との差は、フレームを `Vec<u8>` に書く写しと、受け取り側で画像の領域へ読む写しなど。

## 11. 共有メモリ(Step 3、#492)

同じホストの別プロセスへは、大きなフレームを Zenoh の共有メモリで送る。juiz は Zenoh の領域にフレームを書くだけで、相手に領域の場所を渡すか TCP で中身を送るかは Zenoh が決める。

### 11.1 feature

- juiz_zenoh_broker の feature `shared-memory = ["zenoh/shared-memory", "zenoh/unstable"]`。`default = ["plugin", "shared-memory"]` で、既定オンにする。
- juiz_app の feature `zenoh-broker` は `juiz_zenoh_broker/shared-memory` を含める(juiz_app は `default-features = false` で依存するので、書かないと入らない)。juiz_py の `zenoh-broker` も juiz_app を通して入る。
- 共有メモリを入れるための追加の依存は zenoh-shm などわずかで、Zenoh 全体のビルド時間(§2.1)に比べて小さい。
- feature を切っても(`default-features = false, features = ["plugin"]`)TCP だけでビルドでき、conf の `shared_memory` は無視される。Zenoh の版を上げて共有メモリの API が壊れたときの逃げ道にする(§11.6)。
- feature の有無で `BrokerProxy` などプラグインとの境界の型は変わらない(§8.1、#450)。切り替えは `brokers/zenoh_broker/src/shm.rs` の中だけ。

### 11.2 送り方

- フレーム全体(先頭 `JZF1`・ヘッダ・本体)を、共有メモリの領域 1 つにそのまま書く。その領域を `ZBytes` にして query / reply の payload にする。受け手は TCP のときと同じく `ZBytes` の `reader()` でフレームを読む(§5.2)。
  - large-data.md §6 の共有メモリ(B、`body_len` 0 の本体)は使わない。B は juiz がリングと寿命を管理するが、Zenoh では領域の参照数・回収・watchdog を Zenoh が管理するので、juiz の側で管理する物が無い。同じ System で IPC の B と Zenoh の共有メモリを両方使ってよい。
- 書く前にフレームの長さ(8 + ヘッダ + 本体ごとに 8 + 本体)を求め、その大きさの領域を借りて `OutgoingFrame::write_to` で直接書く。画像はカプセルから領域へ 1 回だけ写す(TCP のときの `Vec<u8>` への写しが無くなる)。
- 領域は Zenoh のセッションが持つ provider(`Session::get_shm_provider()`、transport_optimization の pool)から借りる。セッションを開いた後、provider の用意ができるまで待つ。
- 64 KiB より小さなフレームは共有メモリに書かず `Vec<u8>` で渡す(領域を借りる手間の方が大きいため)。このとき Zenoh の transport_optimization が 3072 バイト以上のメッセージを自分で共有メモリへ写すことはある(Zenoh の既定)。
- 借り方は `GarbageCollect<Defragment>` で、空きが無ければ 1 ms ごとに最大 1 秒試す。それでも空かないとき、pool(`shm_pool_bytes`)より大きいフレーム、provider が無いときは、警告を出して `Vec<u8>` で送る(エラーにしない)。Zenoh の `BlockOn` は使わない。pool より大きいフレームで永久に待つため。
- 応答のフレーム(サーバー)も要求のフレーム(クライアント)も同じ規則で書く。Topic の push も要求なので同じ。
- 相手が別のホスト、または相手が共有メモリを切っているときは、Zenoh がセッションを開くときにそれを確かめ、領域の中身を TCP で送る。juiz の側は何もしない。

### 11.3 conf

ブローカーとブローカープロキシの両方に書ける(§4.1、§4.2)。

| キー | 既定 | 意味 |
|---|---|---|
| `shared_memory` | `true` | `false` で Zenoh の `transport/shared_memory/enabled` を `false` にし、フレームを共有メモリに書かない。片方が `false` なら、その間は TCP で送る |
| `shm_pool_bytes` | 67108864(64 MiB) | Zenoh の `transport/shared_memory/transport_optimization/pool_size`。4K の RGB(約 25 MB)のフレームが 2 つ入る大きさ。これより大きなフレームは TCP で送る |

- pool はセッションごと(サーバーは 1 つ、クライアントは繋ぎ先ごとに 1 つ)にでき、`/dev/shm` などの共有メモリを使う。Docker の既定の `/dev/shm`(64 MB)では足りないことがあるので、`--shm-size` を増やすか `shm_pool_bytes` を下げる。

### 11.4 テスト

- `tests/zenoh_shm_test.rs`(ポート 5253・5254): 約 6 MB の画像を戻り値と引数で送り、両方が共有メモリを使うとき、クライアントだけ切ったとき、クライアントの pool がフレームより小さいとき、サーバーだけ切ったときに、中身が変わらず届くこと。サーバーの pool をフレーム 2 つ分(13 MiB)にして 10 往復させ、返された領域が使い回されること。
- 既定で共有メモリがオンなので、§10.1 の同じプロセス・別プロセスのテストも、64 KiB 以上のフレームは共有メモリを通る。

### 11.5 計測

`zenoh_bench_test.rs` と `image_transfer_bench`(IPC)を同じクラウドの環境(4 vCPU、release、2026-11-14)で続けて測った値(1 回あたり [ms]、Topic の push は受け手の Process の実行が終わって応答が返るまで):

| 大きさ | 方式 | call の戻り値 | 引数で送る | Topic の push |
|---|---|---|---|---|
| 720p | Zenoh 共有メモリ(§11.2) | 1.2〜1.4 | 1.2〜1.4 | 1.1〜1.6 |
| 720p | Zenoh 共有メモリ(feature だけ。juiz は `Vec<u8>` で渡し、Zenoh が pool へ写す) | 1.9 | 2.1 | 1.9 |
| 720p | Zenoh TCP(feature なし) | 2.4 | 2.3 | 2.4 |
| 720p | IPC | 0.8 | 1.7 | 1.0(B、届くまで) |
| 720p | qmp | 6.5 | 7.3 | – |
| 4K | Zenoh 共有メモリ(§11.2) | 8.1〜8.9 | 7.4〜8.2 | 9.5〜10.7 |
| 4K | Zenoh 共有メモリ(feature だけ) | 23.0 | 24.4 | 24.6 |
| 4K | Zenoh TCP(feature なし) | 19.2 | 22.4 | 21.7 |
| 4K | IPC | 9.1 | 100.2 | 11.5(B、届くまで) |
| 4K | qmp | 66.3 | 62.8 | – |

- 共有メモリは TCP より 720p で約 1.8 倍、4K で約 2.5 倍速い。4K の call の戻り値は IPC と同じくらいで、引数で送るのは IPC の 10 倍以上速い。
- feature を入れるだけ(Zenoh の transport_optimization に任せる)では、720p は 2.4 → 1.9 ms にしかならず、4K は Zenoh の既定の pool(16 MiB)に入らないので速くならない。フレームを領域へ直接書くことと pool を広げることの両方が要る。
- 残りの時間の大半は、送り手の画像から領域への写しと、受け手で領域から画像の `Vec` への写し(4K でそれぞれ約 2.5 ms)。受け手の写しを無くすには、画像のカプセルが共有メモリを指せるようにする必要があり、ここではしない。
- 引数で送るときの値には、ベンチの中での画像の複製(4K で約 2.6 ms)も入っている。
- juiz の外で測った値(§1、4K 3.0 ms)は、同じ領域を送り続けるだけで受け手は中身を写さないので、そのまま比べられない。

### 11.6 リスク

- Zenoh の共有メモリの API(`zenoh::shm`、`Session::get_shm_provider`)は `unstable` で、Zenoh の版を上げると変わることがある。zenoh は 1.10.2 に固定しておき、上げるときは `shm.rs` を直す。直せないときは feature `shared-memory` を外して出せる(§11.1)。
- Zenoh の既定の設定の説明に、CPU の負荷が高いと送っている途中の共有メモリの領域が watchdog に無効にされる競合が query / reply にあると書かれている(eclipse-zenoh/zenoh#2628)。起きると受け手がフレームを読めず、エラー応答か応答なしになる。起きる環境では `shared_memory: false` にする。
- 異常終了したプロセスの領域は `/dev/shm` に残るが、Zenoh は次に共有メモリを使い始めるときに持ち主のいない領域を消す。
- Windows・macOS では、CI の zenoh ジョブ(Linux)では確かめていない。公開する wheel(§8.3)の組み立てが main への push でビルドを確かめる。

## 12. 後で決めること

- Topic を Zenoh の put / subscribe にするか。Step 1 の push(query / reply)の数字を見て、別の issue で決める。そのときは key expression `juiz/v1/topic/<ns>/<topic 名>` を使う。

## 13. 対象外

- Zenoh のルーター(zenohd)を挟む構成、複数の System が 1 つの Zenoh 網に入る構成(§5.1 の固定の key が誤配しないことを前提にしているため)。`zenoh_config` で組めても動作は保証しない。
- scouting での相手の自動発見。
- TCP 以外の transport、TLS・認証。
- pyjuiz・C++・Haskell のクライアント(pyjuiz は HTTP・IPC を使う)。
- ROS 2(rmw_zenoh)との相互接続。

## 14. Step との対応

| Step | issue | この文書の節 |
|---|---|---|
| 0 仕様書 | #489 | 全体 |
| 1 クレート・TCP の call / Topic・テスト・ベンチ | #490 | §2〜§7、§8.1、§10 |
| 2 juiz_app / juiz_py への組み込み、publish.yml、CI の別ジョブ | #491 | §2.1、§8.1、§8.2、§8.3 |
| 3 共有メモリ | #492 | §11、§4 の `shared_memory`・`shm_pool_bytes` |
| 4 公式サイトのガイド | OpenJUIZ.github.io#96 | §4 |
| 5 リリース | #493 | – |
