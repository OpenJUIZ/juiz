# Zenoh ブローカー仕様

- 状態: 確定(Zenoh ブローカー Step 0、issue #489、2026-11-14)
- 親 issue: #487。計画と経緯はプロジェクトの計画書 zenoh-broker-plan.md(2026-11-14)にある
- 関連: フレーム形式は [large-data.md](large-data.md) §3、qmp の決めごとは同 §4、プラグインの境界は [rust-plugin-abi.md](rust-plugin-abi.md) §12(#450)、似た作りの [ipc-broker.md](ipc-broker.md)
- この文書は Step 1〜5(#490〜#493、site#96)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と範囲

別プロセス・別ホストの juiz を、qmp(QUIC)より速く呼び出すためのブローカー。種類名は `zenoh`。

- 通り道は [Zenoh](https://zenoh.io/) 1.x の TCP。同じホストでは Step 3 で Zenoh の共有メモリを足す(§11)。
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
- 依存は `zenoh = { version = "1.10.2", default-features = false, features = ["transport_tcp"] }`。ほかの transport(UDP・QUIC・WebSocket など)は入れない。共有メモリの feature は Step 3 で足す(§11)。
- Zenoh のライセンスは EPL-2.0 OR Apache-2.0。juiz からは Apache-2.0 として使う。
- バージョンは他のクレートと同じ(ワークスペースの版)。crates.io への公開は Step 2 で publish.yml に足す。

### 2.1 ワークスペースと CI 時間

Zenoh を足すと juiz_app の初回ビルドが 86 秒から 190 秒になる(4 コア debug、依存 419 → 576 クレート)。既存の CI の時間を変えないため、次を守らなければならない。

- `brokers/zenoh_broker` はワークスペースの `members` に入れ、`default-members` には入れない。`scripts/test.sh` の `cargo build` / `cargo test` と、カバレッジのジョブ(`default-members` からパッケージを選ぶ)は Zenoh をビルドしない。
- juiz_app の feature `zenoh-broker` は `default` に入れない(§8.2)。
- juiz_core に `juiz_zenoh_broker` を dev-dependency として足さない(qmp は `image_transfer_bench` のために足しているが、そうすると juiz_core のテストが Zenoh をビルドする)。Zenoh のテストとベンチは `brokers/zenoh_broker/tests` に置く(§10)。
- CI は Zenoh だけの別ジョブにする(Step 2 #491)。それまで Step 1 のテストは手元で `cargo test -p juiz_zenoh_broker -- --test-threads=1` で走らせる。

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

### 8.2 juiz_app

- feature `zenoh-broker = ["dep:juiz_zenoh_broker"]`。`default` に入れない。有効なとき、`main_with_args` で `zenoh_broker::register()` を呼ぶ。
- 同じ `type_name` のプラグインを `plugins.broker_factories.zenoh_broker` で読んだときは、qmp と同じくプラグインを優先する。
- feature が無い juiz で conf に `type_name: zenoh` を書いたときは、今の「知らないブローカーの種類」のエラーに、`--features zenoh-broker` でビルドするかプラグインを読むよう案内を足す(Step 2)。
- pip の juiz-cli(juiz_py)の wheel だけ feature をオンにするかは Step 2 で決める。

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

## 11. 共有メモリ(Step 3)

同じホストの別プロセスへは、Zenoh の共有メモリ(feature `shared-memory` + `unstable`)で本体を送る。Step 3 で決めること:

- juiz_zenoh_broker の feature 名と、juiz_app の feature `zenoh-broker` に含めるか。
- 確保は `BlockOn<GarbageCollect>` の policy で待つ(そうしないと `OutOfMemory` になる。juiz の外の計測で確かめた)。
- フレームのヘッダと本体の置き方(フレーム全体を共有メモリに書くか、本体だけか)。
- large-data.md §6 の共有メモリ(B、IPC の Topic)との関係。

## 12. 後で決めること

- Topic を Zenoh の put / subscribe にするか。Step 1 の push(query / reply)の数字を見て、別の issue で決める。そのときは key expression `juiz/v1/topic/<ns>/<topic 名>` を使う。
- juiz-cli の wheel で feature をオンにするか(Step 2)。

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
| 2 juiz_app / juiz_py への組み込み、publish.yml、CI の別ジョブ | #491 | §2.1、§8.2 |
| 3 共有メモリ | #492 | §11 |
| 4 公式サイトのガイド | OpenJUIZ.github.io#96 | §4 |
| 5 リリース | #493 | – |
