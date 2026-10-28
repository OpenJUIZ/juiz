# IPC ブローカー仕様

- 状態: 確定(大きなデータ Step 2、issue #232、2026-10-28)
- 親 issue: #229。計画と経緯はプロジェクトの計画書 ipc-broker-review.md(2026-10-28 確定)にある
- 関連: 大きなデータの仕様 [large-data.md](large-data.md)(フレーム形式と共有メモリ)
- この文書は #233(IPC ブローカーの作り直し)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と範囲

同じマシンの別プロセスの juiz を、ポートを開けずに、速く呼び出すためのブローカー。種類名は `ipc`。

- 通り道は Unix ドメインソケット(Linux / macOS)と名前付きパイプ(Windows)。
- 要求と応答の形は large-data.md §3 のフレーム。画像は PNG にせず生バイトで送る。
- 同じマシンの中だけで使う。別マシンへは http か qmp を使う。
- アクセス制御はファイル(パイプ)の権限でする。同じユーザーのプロセスだけが繋げる。

今の `juiz_core/src/brokers/ipc` は試作で、登録されておらず動かない(計画書 §1)。この仕様に沿って作り直す。

## 2. 通り道と名前

### 2.1 ブローカーの名前

- ブローカーの名前(conf の `name`)は `[A-Za-z0-9_-]` と `.` からなる 1〜64 文字で、`.` で始まってはならない。違えば起動時にエラーにする。
- 省略したときは `juiz` にする。
- 名前はソケットの場所と ID(§3)の両方を決める。サーバーとクライアントは、名前だけから同じ場所を求めなければならない。

### 2.2 Unix(Linux / macOS)のソケットの場所

`<dir>/<name>.sock`。`<dir>` は次の順で最初に決まったもの。

1. 環境変数 `JUIZ_IPC_DIR`(コンテナでボリュームを共有するときなど)
2. `$XDG_RUNTIME_DIR/juiz`
3. `/tmp/juiz-<uid>`(`<uid>` は実行ユーザーの数値の ID)

- `<dir>` が無ければ、モード 0700 で作る。
- 3 の `/tmp/juiz-<uid>` がすでにあり、所有者が自分でないかモードが 0700 でなければ、エラーにする(他人が用意した場所に繋がないため)。
- ソケットのファイルはモード 0600 にする。
- パスがソケットの長さの上限(macOS 104 バイト、Linux 108 バイト)を超えたら、`JUIZ_IPC_DIR` で短い場所を指定するよう案内してエラーにする。

### 2.3 Windows の名前付きパイプ

`\\.\pipe\juiz-<name>`。作ったユーザーだけに許すセキュリティ記述子を付けなければならない(既定の記述子は Everyone に読み取りを許すため)。

### 2.4 実装の目安

- Unix は `std::os::unix::net::{UnixListener, UnixStream}` を使うのがよい。読み取りのタイムアウトが標準で使え、共有メモリ(large-data.md §6.6)で記述子を `SCM_RIGHTS` で渡せる。
- Windows は `interprocess` の名前付きパイプを使ってよい。

### 2.5 残ったソケット

前のサーバーが異常終了すると、ソケットのファイルが残る。

- 起動時にファイルがあれば、繋いでみる。繋がらなければ(接続を拒まれる)ファイルを消して作り直す。繋がれば、同じ名前のサーバーが動いているので、エラーにする(`ipc://<name> is already in use`)。
- 止めるときはソケットのファイルを消す。

## 3. ID

- ブローカーの ID は `ipc://<name>`。オブジェクトの ID は http・qmp と同じく `ipc://<name>/process/<ns>/<name>::<type>` の形になる。
- サーバーは qmp と同じく、要求の `identifier` が自分を指す `ipc://<自分の名前>/...` なら `core://core/...` に書き換え、`param` に `accessed_broker_id`(`ipc://<name>`)・`accessed_broker_type`(`ipc`)・`accessed_broker_name` を入れる(`localize_identifier`)。
- IPC には相手のネットワークアドレスが無いので、`requesting_host_addr` は付けない(`on_update` の `opt_requesting_host_addr` は `None`)。

## 4. conf と CLI

### 4.1 conf

```yaml
brokers:
  - type_name: ipc
    name: cam0                  # 省略すると juiz
    max_frame_bytes: 1073741824 # 1 フレームの上限 (large-data.md §3.5)。省略してよい
```

- 今の manifest の `namespace` キー(ソケットのファイル名)は、ID のネームスペースと紛らわしいので廃止し、`name` で決める。`namespace` が書いてあれば、`name` を使うよう案内してエラーにする。
- `buffer_size` も廃止する(フレームは長さ付きなので要らない)。
- `subsystems:` / `mastersystem:` の接続先も、http・qmp と同じく `ipc://<name>` で書ける。

### 4.2 CLI

- サーバー: `juiz -d --ipc <name>` で IPC ブローカーを起動する。http ブローカーは今の `-d` の規則のまま(`-q` で止める)。
- クライアント: `juiz -s ipc://<name> process list` のように、`-s` に ID を渡す。

## 5. 接続

### 5.1 サーバー

- 接続を受け付けるたびに、その接続用の普通のスレッドを作る(tokio のタスクにしない。処理中に BrokerProxy や EC のランタイムを作ったり捨てたりしたときの panic を避けるため、#231 と同じ理由)。
- 接続のスレッドは、相手が閉じるまで、フレームを読む → 処理する → 応答のフレームを書く、を繰り返す。1 つの接続の要求は順に処理する。
- 処理は `CRUDBroker` の振り分け(`on_value_request` と同じ)に、`CapsuleMap` を直接渡して行う(large-data.md §3.2)。
- 処理が失敗したら、必ずエラー応答(large-data.md §3.2 の `kind: "error"`)を返し、接続は保つ。
- 接続の数の上限は設けない(同じユーザーのプロセスだけが繋ぐため)。

### 5.2 クライアント(`IpcBrokerProxy`)

- `MessengerBrokerProxy` は使わず、qmp の BrokerProxy と同じく、要求ごとに `param` と `map` を組み立てる専用の BrokerProxy にする(`method_name` は小文字)。
- 接続は最初の要求のときに作り、以後使い回す。要求は 1 つずつ送る(応答を受け取るまで次を送らない)。
- 要求を書き終わる前に失敗したら(繋がらない、相手が閉じていた)、接続を作り直して 1 回だけ送り直す。
- 要求を書き終わった後に失敗したら(応答の途中で切れた、タイムアウト)、送り直さずにエラーを返し、接続を捨てる。次の要求で作り直す。サーバーで処理が済んだかどうかわからないため。
- 応答の待ち時間の上限は既定 10 秒。BrokerProxy の manifest の `timeout_ms` で変えてよい。
- 応答の `id` が要求と合わなければ、エラーにして接続を捨てる。

## 6. 組み込み

- juiz_core に feature `ipc-broker` を足す。有効なとき、`register_builtin_broker_factory` で IPC の BrokerFactory と BrokerProxyFactory を登録する。`interprocess` はこの feature のときだけ依存する。
- juiz_app で既定で有効にするかは #233 で決める(qmp と同じく既定で有効にするのがおすすめ。追加の依存が小さいため)。

## 7. 今の実装から変えるもの

| 今 | 変えた後 |
|---|---|
| `System::setup()` での登録がコメントアウトされている | feature `ipc-broker` で登録する(§6) |
| サーバーは 1 接続で 1 要求だけ読んで切り、クライアントは接続を使い回す | どちらも接続を使い回す(§5) |
| サーバーは manifest の `namespace`、クライアントは `object_name` でソケットを決めるので一致しない | どちらも名前から §2 の場所を求める |
| `/tmp/<名前>` 固定で Windows で動かない | §2.2・§2.3 |
| 1 行 1 JSON | large-data.md §3 のフレーム |
| 失敗すると応答を返さずに切る | エラー応答を返す(§5.1) |
| `println!` が残っている、`ipc_broker_.rs` は使われていない | 消す |

## 8. テスト(#233)

- `brokers/qmp_broker/tests/qmp_broker_api_test.rs` を雛形に、同じプロセスの中でのブローカー越しの呼び出しと、子プロセス(`juiz -d --ipc`)への呼び出しの両方。
- 画像(720p の `rgb8` と、`l16`・`rgba32f` など形式ごと)の引数と戻り値、数値の配列(`i64le`・`f64le`、混ざった配列が JSON のまま届くこと)、Topic の push で画像を送ること。
- エラー応答、接続が切れた後の再接続、残ったソケットの片付け、名前の検証。
- CI で Linux / macOS / Windows。
- #230 のベンチに IPC を足して、large-data.md §8 の目安を確かめる。

## 9. 対象外

- 別マシンとの通信。
- 他のユーザーのプロセスからの接続。
- 他言語のクライアント(pyjuiz の IPC クライアントは #236 で検討する)。
