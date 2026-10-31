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
- #233 の実装: Unix は std の `UnixListener` / `UnixStream`、Windows は `interprocess` の local socket(セキュリティ記述子は SDDL `D:P(A;;GA;;;OW)(A;;GA;;;SY)`、作ったユーザーと SYSTEM だけ)。通り道の違いは `juiz_core/src/brokers/ipc/transport.rs` に閉じ込めてある。Windows でサーバーを止めるときは、受け付けた接続を `DisconnectNamedPipe` で切り、`CancelIoEx` で読み取り待ちを取り消す。

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
- `buffer_size` も廃止する(フレームは長さ付きなので要らない)。書いてあっても警告をログに出して無視する(エラーにはしない)。
- `subsystems:` / `mastersystem:` の接続先も、http・qmp と同じく `ipc://<name>` で書ける。

### 4.2 CLI

- サーバー: `juiz -d --ipc <name>` で IPC ブローカーを起動する。http ブローカーは今の `-d` の規則のまま(`--no-http` で止める)。
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
- 応答の待ち時間の上限は既定 10 秒。BrokerProxy の manifest の `timeout_ms` で変えてよい。Windows の名前付きパイプには読み取りのタイムアウトが無いので、今は待ち時間の上限を設けない(#233 の時点の制限)。
- 使い回す接続は、要求を書く前に相手が閉じていないかを確かめ(Unix はノンブロッキングの読み取りで EOF を見る、Windows は `PeekNamedPipe` が失敗するかを見る)、閉じていれば作り直す。サーバーを再起動した後の最初の要求も、送り直しなしで届く。
- 応答の `id` が要求と合わなければ、エラーにして接続を捨てる。

## 6. 組み込み

- juiz_core に feature `ipc-broker` を足す。有効なとき、`register_builtin_broker_factory` で IPC の BrokerFactory と BrokerProxyFactory を登録する。`interprocess` はこの feature のときだけ依存する。
- juiz_core の既定の feature に含め、juiz_app も feature `ipc-broker` で既定で有効にする(#233 で決定)。Unix では追加の依存は無い(`libc` だけ)。`interprocess` は Windows で feature が有効なときだけ依存する。

## 7. 今の実装から変えるもの(#233 で変えた)

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
- pyjuiz 以外の言語のクライアント(pyjuiz は #236 で足した、§10)。

## 10. pyjuiz の IPC クライアント(#236 で決めたこと)

`juiz.proxy.IpcProxy`(`bindings/pyjuiz/juiz/proxy/ipc.py` と `__init__.py`)。`HttpProxy` と同じ `CRUDProxy` なので、`SystemProxy` / `ProcessProxy` / `ContainerProxy` の上の使い方は HTTP と変わらない。

```python
from juiz.proxy import ProcessProxy, ContainerProxy, connect

cam = ProcessProxy("capture0", ipc="cam0")    # ipc://cam0 に繋ぎ、名前で探す
img = cam.call()                              # 画像は PNG を経由せず画素のまま届く
system = connect("ipc://cam0")                # SystemProxy。connect("http://localhost:8000") も同じ形
```

- 名前と場所: §2.1 の名前の検証と、§2.2 の場所(`JUIZ_IPC_DIR` → `$XDG_RUNTIME_DIR/juiz` → `/tmp/juiz-<uid>`)を Rust と同じ順で求める。クライアントなのでディレクトリは作らない。Windows は `\\.\pipe\juiz-<name>` をファイルとして開く(読み取りのタイムアウトは無い。Rust の BrokerProxy と同じ制限)。
- 接続: §5.2 と同じ。最初の要求で繋いで使い回し、要求は 1 つずつ(スレッドからはロックで順に)送る。書く前に相手が閉じていれば(`MSG_PEEK` で EOF)繋ぎ直し、書き終わる前の失敗は 1 回だけ送り直し、書き終わった後の失敗は送り直さずに接続を捨てる。待ち時間の上限は既定 10 秒(`IpcProxy(name, timeout=秒)`)。`close()` と `with` で閉じられる。
- 要求: `param` に `method_name`(小文字)・`class_name`・`function_name`・`accessed_broker_name` と、HTTP のクエリにあたるもの(`identifier` など)を入れる。create の `{"__map__": ..., "__param__": ...}` は中身を取り出す(Rust の `IpcBrokerProxy` と同じ)。
- 画像を送る: PIL の `Image`(`L`→`l8`、`LA`→`la8`、`RGB`→`rgb8`、`RGBA`→`rgba8`、`I;16`→`l16`。ほかのモード(`P` など)は `RGB` か `RGBA` にしてから)と、numpy の配列(`uint8` / `uint16` の 1〜4 チャンネル、`float32` の 3・4 チャンネル。高さ × 幅 [× チャンネル])。numpy の配列は連続でなければ詰めてから送る。
- 画像を受け取る: PIL で表せる `l8` / `la8` / `rgb8` / `rgba8` / `l16` は PIL の `Image`(HTTP と違い `RGB` に変換しない)、それ以外(`la16` / `rgb16` / `rgba16` / `rgb32f` / `rgba32f`)は numpy の配列。numpy が無ければエラー。余白のある `stride` は取り除く。
- 数の配列: large-data.md §3.4 の規則で、1024 要素以上の整数だけ(`bool` は含めない)・小数だけの `list` を `i64le` / `f64le` の本体にし、受け取った本体は `list` に戻す。
- エラー応答は `IpcRemoteError`(`InternalServerError` の子。`class_name`・`function_name` を持つ)、繋がらない・フレームが読めないなどの通り道の失敗は `IpcError`。
- 共有メモリ(B)は使わない。B は Topic の push で送り手が Rust のときだけで、pyjuiz はサーバーにならないため。
- あわせて直したもの: `ProcessProxy.call` / `ContainerProcessProxy.call` に位置で 2 つ以上の引数を渡すと、画像以外を JSON の文字列にしていたので、HTTP の PATCH では文字列のまま届いていた。値のまま渡し、HTTP の multipart(画像があるとき)で画像以外を JSON の文字列にするのは `HttpProxy` が受け持つようにした(`datatype.typed_call` も同じ)。
- テスト: `bindings/pyjuiz/test/ipc_frame_test.py`(サーバーなしでフレームの往復、余白のある画像、不正なフレーム、偽のサーバーでのエラー応答と画像)と、`juiz_core/tests/integration/pyjuiz_ipc_proxy_test.rs`(System を起動して `bindings/pyjuiz/test/ipc_proxy_test.py` を別プロセスで実行。Process / Container、画像の形式ごとの引数と戻り値、数の配列、エラー応答、繋ぎ直し、HTTP との比べ)。
