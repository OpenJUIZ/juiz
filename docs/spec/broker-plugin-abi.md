# ブローカーのプラグインの C ABI 仕様

- 状態: 確定(ブローカーの C ABI 化 Step 0、issue #635、2026-11-27)
- 親 issue: #634。計画と経緯はプロジェクトの計画書 zenoh-broker-repo-plan.md(2026-11-26 確定)にある
- 関連: フレーム形式 [large-data.md](large-data.md) §3、Process などの C ABI [rust-plugin-abi.md](rust-plugin-abi.md)(形をそろえる。§12 の表をこの文書で直した)、Zenoh ブローカー [zenoh-broker.md](zenoh-broker.md)(Step 2 で載せ替える)、アプリ配布形式 [app-package.md](app-package.md) §6.3・§12.1、`juiz sdk install`(juiz_app の `sdk.rs`、#548)
- この文書は Step 1〜7(#636〜#641、site#127)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と用語

ブローカーを、juiz と違う rustc・違う juiz_sdk の版でビルドしても読み込めるようにする。そうすると、ビルド済みのブローカーを OS ごとのアーカイブで配り、`juiz broker install <名前>` で後から入れられる。juiz 本体には Zenoh などの重い依存を入れずに済む。

今のブローカーのプラグインは Rust ABI で、`BrokerFactory` / `BrokerProxyFactory` の trait と `CoreBrokerPtr` をそのまま受け渡している(rust-plugin-abi.md §12)。ホストと同じ rustc・juiz_sdk の版でなければ読めない。

一方、qmp・Zenoh のサーバーとプロキシがしていることは、どちらも **JZF1 フレーム(large-data.md §3)を 1 つ送って 1 つ受け取る** ことだけで、フレームの組み立て・読み取り・自分宛ての ID の書き換え・CRUDBroker への振り分けは種類によらず同じ。そこで、その共通の部分をホスト(juiz_core)に置き、プラグインには **フレームのバイト列を運ぶことだけ** を任せる。

| 用語 | 意味 |
|---|---|
| C ABI のブローカー | この文書の入口(`broker_factory_cabi`、§3)を持つプラグイン |
| Rust ABI のブローカー | 今の `broker_factory` / `broker_proxy_factory`(`extern "Rust"`)だけを持つプラグイン |
| ホスト | ブローカーを読み込む juiz(juiz_core) |
| サーバー | 要求を受けて応答を返す側。今の `Broker`(`CRUDBrokerHolder`) |
| クライアント | 要求を送って応答を受け取る側。今の `BrokerProxy`(`CRUDBrokerProxyHolder`) |
| フレーム | large-data.md §3.1 のバイト列そのもの(先頭の `"JZF1"`・長さ・ヘッダ・本体)。プラグインは中を読まない |
| 読み口 | プラグインが用意し、ホストがフレームのバイト列を引き出す口(`juiz_frame_reader`、§4) |
| 書き口 | プラグインが用意し、ホストがフレームのバイト列を書き込む口(`juiz_frame_writer`、§4) |
| グルー | juiz_sdk の中の、プラグイン側の C の入口と Rust の trait をつなぐ部分(§8) |
| ブローカーの ABI の版 | この文書の入口の表と関数の形の版。最初は `1`(§9)。rust-plugin-abi.md の版とは別に数える |
| 置き場所 | `juiz broker install` が入れる場所 `~/.juiz/brokers/<名前>/<版>/`(§10) |

## 2. 基本方針

1. **境界はバイト列と C の関数だけ。** rust-plugin-abi.md §2 の 1 と同じ。Rust の型(`Value`・`CapsuleMap`・`JuizResult`・trait object)は境界を越えない。manifest は JSON のバイト列で渡す。
2. **プラグインはフレームを運ぶだけ。** フレームを作る・読む、`localize_request`(自分宛ての ID の書き換え、§6.3)、`CRUDBroker` への振り分け、エラー応答のフレームを作ること、id の照合は、ホストの共通の処理にする。プラグインはフレームの形を知らなくてよい(長さだけを知る)。
3. **境界でコピーを増やさない。** 今の qmp・Zenoh と同じ回数(送り手の値から通り道へ 1 回、通り道から受け手の最終的な領域へ 1 回)に収める。受け取りは、ホストが最終的な領域を用意してプラグインにそこへ直接書かせる(読み口、§5.1)。送り出しは、ホストがプラグインの用意した領域(共有メモリなど)へ直接書く(書き口、§5.2)。
4. **プラグインは juiz_core に依存しない。** グルーは juiz_sdk に置く。juiz_core の feature にも rustc にも縛られない。
5. **ホストはブローカーの ABI の版の数字だけを比べる。** rustc と juiz_sdk の版は比べない(ログに出すだけ)。
6. **panic は境界を越えない。** 両側とも、相手から呼ばれる関数の中身を `catch_unwind` で囲む(§7.2)。
7. **要求と応答だけ。** サーバーから相手へ一方的に送る形(Zenoh の put / subscribe など)は ABI に入れない(§13)。Topic の push は今までどおり要求・応答で運ぶ。
8. **今の Rust ABI のブローカーも当面読める。** qmp のプラグインなど(§11)。

## 3. 入口

### 3.1 シンボル

C ABI のブローカーは、cdylib に次の 1 つのシンボルを置く。

```rust
#[no_mangle]
pub extern "C" fn broker_factory_cabi() -> *const JuizBrokerEntry;
```

- 引数を取らず、プラグインの中の `static` な入口の表(§3.2)へのポインタを返す。表はプラグインを外すまで有効でなければならない。null を返してはならない。
- 1 つの cdylib に置けるブローカーの種類は 1 つ。種類名は `info` の `type_name`(§3.3)。
- 移行のあいだ、同じ cdylib に Rust ABI の `broker_factory` / `broker_proxy_factory` / `broker_factory_build_info` を並べて置いてよい(§11)。

### 3.2 入口の表

```c
/* プラグイン側が static に置く。並びと型を変えるときは ABI の版を上げる(§9.1) */
typedef struct juiz_broker_entry {
    uint32_t abi_version;   /* この表の ABI の版。今は 1 */
    uint32_t struct_size;   /* sizeof(juiz_broker_entry)。後ろに項目を足すときに使う */
    const char* abi_marker; /* "juiz_broker_abi=1;"(§9.3) */

    /* プラグインの情報の JSON を out に書く(§3.3)。ほかのどの関数より先に呼んでよい */
    int64_t (*info)(const juiz_bytes_sink* out);
    /* 読み込んだ直後に 1 回だけ呼ぶ。ホストのログの口などを渡す(§3.4) */
    int64_t (*init)(const juiz_broker_host_services* services, const juiz_bytes_sink* error);

    /* サーバー(§5.3) */
    int64_t (*server_start)(juiz_bytes manifest_json, const juiz_broker_server_host* host,
                            void** handle, const juiz_bytes_sink* name,
                            const juiz_bytes_sink* error);
    void (*server_stop)(void* handle);

    /* クライアント(§5.4) */
    int64_t (*client_connect)(juiz_bytes manifest_json, void** handle,
                              const juiz_bytes_sink* error);
    int64_t (*client_request)(void* handle, const juiz_frame_source* request,
                              const juiz_frame_receiver* response,
                              const juiz_bytes_sink* error);
    void (*client_close)(void* handle);
} juiz_broker_entry;
```

- 関数ポインタはすべて null であってはならない。ホストは null があれば読み込みをエラーにする(`BrokerPluginLoadError`、§7.3)。サーバーだけ・クライアントだけのブローカーは今は作らない(片方しか無いと、その種類の ID を使った呼び出しの半分が通らないため)。
- ホストは `abi_version` を最初に読み、知らない版なら残りを読まずにエラーにする(§9.2)。
- `struct_size` が、ホストの知っている大きさより小さければエラー、大きければ知っている部分だけを使う。
- Rust では `#[repr(C)] pub struct JuizBrokerEntry` として juiz_sdk の `juiz_sdk::broker_cabi` に置く(Step 1)。`juiz_bytes`・`juiz_bytes_sink` は rust-plugin-abi.md §4 と同じもの(`juiz_sdk::cabi` の型)を使う。

### 3.3 `info` の JSON

```json
{
  "type_name": "zenoh",
  "abi_version": 1,
  "plugin_name": "juiz_zenoh_broker",
  "plugin_version": "0.1.0",
  "build_info": "juiz_sdk=0.3.0;rustc=rustc 1.91.0 (f8297e351 2025-10-28)",
  "default_port": 7448
}
```

| キー | 必須 | 意味 |
|---|---|---|
| `type_name` | ○ | ブローカーの種類名。conf の `type_name` と ID の頭(`zenoh://…`)になる。英小文字・数字・`_` で、英小文字で始まる。組み込みの `http`・`ipc`・`qmp` と同じ名前でもよい(§6.4 の順で決まる) |
| `abi_version` | ○ | 表の `abi_version` と同じ値。違えばエラー。道具が表を読まずに版を知るために置く |
| `plugin_name` | ○ | プラグインの名前(クレート名など)。ログと `juiz broker list` に出す |
| `plugin_version` | ○ | プラグイン自身の版(semver)。juiz の版とはそろえない |
| `build_info` | | `juiz_sdk::build_info` の文字列。ホストはログ(debug)に出すだけで比べない |
| `default_port` | | conf にポートを書かないときに使うポート。ホストが仮の名前と案内の文に使う(§5.3) |

知らないキーは無視する。

### 3.4 `init` とホストのサービス

```c
typedef struct juiz_broker_host_services {
    uint32_t abi_version;     /* 1 */
    uint32_t struct_size;
    void* ctx;
    /* level: 1 error / 2 warn / 3 info / 4 debug / 5 trace。target・message は UTF-8 */
    void (*log)(void* ctx, uint32_t level, juiz_bytes target, juiz_bytes message);
    uint32_t max_level;       /* この値より大きい level は送らなくてよい */
} juiz_broker_host_services;
```

- ホストは読み込んだ直後、`info` の後・ほかの関数の前に `init` を 1 回だけ呼ぶ。`services` はプロセスが終わるまで有効。
- グルーは `log` を Rust の `log` クレートのロガーとして据え(`log::set_boxed_logger`、すでに据えてあれば何もしない)、プラグインの中の `log::info!` などをホストのロガーへ流す。Process の C ABI(rust-plugin-abi.md §13)ではまだ流していないが、ブローカーは自分のスレッドで動き、そのエラーが通信の失敗の原因を知る唯一の手がかりになることが多いので、最初から入れる。
- `init` が負の値を返したら、ホストは読み込みをエラーにする。

## 4. 構造体

`juiz_bytes`・`juiz_bytes_sink` は rust-plugin-abi.md §4 と同じ。

```c
/* 読み口。プラグインが用意し、ホストがフレームを引き出す */
typedef struct juiz_frame_reader {
    void* ctx;
    uint64_t len;   /* フレームの長さ(バイト)。わからなければ UINT64_MAX */
    /* buf に最大 cap バイトを書き、書いた数を *n に入れる。*n == 0 はフレームの終わり */
    int64_t (*read)(void* ctx, uint8_t* buf, size_t cap, size_t* n);
} juiz_frame_reader;

/* 書き口。プラグインが用意し、ホストがフレームを書き込む */
typedef struct juiz_frame_writer {
    void* ctx;
    /* bytes を全部書く。足りなければ負の値 */
    int64_t (*write)(void* ctx, juiz_bytes bytes);
} juiz_frame_writer;

/* 送るフレーム。ホストが用意し、プラグインが書き口を渡して書かせる */
typedef struct juiz_frame_source {
    void* ctx;
    uint64_t len;   /* フレームの長さ(バイト)。必ずわかる */
    /* フレームを writer に書く。書き終えるまで戻らない。1 回だけ呼んでよい */
    int64_t (*write_to)(void* ctx, const juiz_frame_writer* writer);
} juiz_frame_source;

/* 受け取ったフレームの渡し先。ホストが用意し、プラグインが読み口を渡す */
typedef struct juiz_frame_receiver {
    void* ctx;
    /* reader からフレームを読み終えるまで戻らない。1 回だけ呼んでよい */
    int64_t (*receive)(void* ctx, const juiz_frame_reader* reader);
} juiz_frame_receiver;

/* サーバーの応答の送り先。プラグインが用意し、ホストが送るフレームを渡す */
typedef struct juiz_frame_sender {
    void* ctx;
    /* frame を送る。プラグインは frame->write_to を中で呼ぶ。1 回だけ呼ぶ */
    int64_t (*send)(void* ctx, const juiz_frame_source* frame);
} juiz_frame_sender;

/* サーバーにホストが渡す表(§5.3) */
typedef struct juiz_broker_server_host {
    uint32_t abi_version;     /* 1 */
    uint32_t struct_size;
    void* ctx;
    /* 要求を 1 つ処理する。request から要求を読み、reply に応答を送ってから戻る */
    int64_t (*handle)(void* ctx, juiz_bytes peer_addr,
                      const juiz_frame_reader* request, const juiz_frame_sender* reply);
} juiz_broker_server_host;
```

- 整数・ポインタの幅は C と同じ(`size_t` は `usize`、`uint64_t` は `u64`、`int64_t` は `i64`)。
- 構造体と、その中のポインタが指す先は、それを渡した関数の呼び出しが終わるまで有効。呼ばれた側は呼び出しの後まで持ってはならない(`juiz_broker_server_host` と `juiz_broker_host_services` だけは例外で、それぞれ `server_stop` が戻るまで・プロセスが終わるまで有効)。
- 関数ポインタは `extern "C"`。どの関数も panic を外に出してはならない(§7.2)。
- 読み口の `read` は、長さが `len` のときに合わせて `len` バイトを返したら `*n == 0` を返す。`len` より多く返してはならない。ホストは `len` を超えて読まない。
- 読み口・書き口の関数は、渡した関数の呼び出しのあいだ、ホストの 1 つのスレッドから順に呼ばれる(同時には呼ばれない)。

## 5. フレームの運び方

### 5.1 受け取り(プラグイン → ホスト): 読み口

受け取ったフレームは、プラグインが読み口(`juiz_frame_reader`)にしてホストに渡す。クライアントの応答(`juiz_frame_receiver::receive`)と、サーバーの要求(`juiz_broker_server_host::handle` の `request`)の 2 か所。

1. ホストは読み口からまず先頭の 4 + 4 バイトとヘッダを読み、ヘッダの `bodies` の説明から、本体ごとに **最終的な** `Vec<u8>` / `Vec<u16>` / `Vec<f32>` / `Vec<i64>` / `Vec<f64>` を確保する(large-data.md §3.6、rust-plugin-abi.md §5.3 の `FrameReceiver` と同じ。#339 の画像の領域の使い回しも効く)。
2. ホストは本体の領域のポインタを `buf` にして `read` を呼び、**プラグインがその領域へ直接書く**。通り道(ソケット・Zenoh の `ZBytes` の断片・共有メモリ)から受け手の最終的な領域への写しの 1 回だけで、境界でのコピーは増えない。
3. ホストは `len`(わかるとき)を見て、上限(large-data.md §3.5、manifest の `max_frame_bytes`)を超えるなら読む前にエラーにする。
4. ホストはフレームを読み終えるか、途中でエラーにすると戻る。途中でやめたとき、残りのバイト列の後始末(ストリームの読み飛ばし・接続を閉じる)はプラグインがする。

プラグインは JZF1 の区切りを知らなくてよい。ストリーム(TCP など)のプラグインは、送るときに `juiz_frame_source::len` を自分の形で前に付け、受け取るときにその長さを `len` にして読み口を作る。

### 5.2 送り出し(ホスト → プラグイン): 書き口

送るフレームは、ホストが `juiz_frame_source` にしてプラグインに渡す。クライアントの要求(`client_request` の `request`)と、サーバーの応答(`juiz_frame_sender::send` の `frame`)の 2 か所。

1. プラグインは `len` を見て送り先の領域を用意する。共有メモリ(Zenoh の `ZShmMut` など)なら `len` バイトを確保する。ソケットならそのまま書いてよい。
2. プラグインは自分の書き口(`juiz_frame_writer`)を作り、`write_to(ctx, writer)` を呼ぶ。
3. ホストは先頭・ヘッダ・各本体の長さと本体を、順に `write` で書く。画像の本体は、カプセルをロックしたままその画素を直接指すバイト列で渡す(rust-plugin-abi.md §5.2 と同じ)。**ホストの値からプラグインの領域への写しの 1 回だけ** で、ホストの中で 1 つのバッファにまとめる複製はしない。
4. `write` の合計はちょうど `len` になる。ホストは `len` と違う長さを書いてはならない。

これが計画の「プラグインが共有メモリの領域を貸して、ホストが書く口」にあたる。領域を確保するのも、その領域を何に使うかもプラグインが決め、ホストは書くだけ。

### 5.3 サーバー

```text
ホスト                                   プラグイン
  | server_start(manifest, host) ------->  listen、受け取りのスレッドを作る
  | <------------------------------- 0 と実際の名前(name)
  |                                        要求が来る(プラグインのスレッド)
  | <--- host->handle(peer, reader, sender)
  |  reader から要求を読む(§5.1)
  |  localize_request(§6.3)、CRUDBroker で処理
  |  sender->send(source) ------------->  source->write_to(書き口)で応答を書く(§5.2)、送る
  | ---- handle が 0 を返す ----------->
  | server_stop(handle) ---------------->  受け取りをやめ、処理中の handle が戻るまで待つ
```

- `server_start` は、待ち受けを始められた(ポートを開けた)ところで戻る。待ち受けに失敗したら負の値とエラー文を返す。今の qmp・Zenoh のように「log に出して何もしない」はしない(app-package.md §16.4 の、`start_brokers` のエラーに理由を載せる方針と同じ)。
- `name` には、実際に待ち受けた住所(`<host>:<port>`、IPv6 は `[<addr>]:<port>`、`0.0.0.0` は `127.0.0.1` に直す)を書く。ホストはそれで `update_broker_name` してから起動済みにする。今の qmp・Zenoh の `on_start` がしていることを、ホストの共通の処理に移す。
- manifest に `name` が無いとき、ホストは `server_start` を呼ぶ前の仮の名前を `<host>:<port>`(`host` の既定は `0.0.0.0`、`port` の既定は `info` の `default_port`)にする。`default_port` も無ければ `<type_name>` にする。
- `handle` はプラグインのスレッドから呼ぶ。**同時にいくつ呼んでもよい**(ホストは要求ごとに並行に処理する、#356)。`handle` は Process の実行が終わるまで戻らないので、プラグインは非同期のランタイムのワーカーの中ではなく、ブロックしてよいスレッドから呼ばなければならない。
- `handle` の中から、ホストは `sender->send` をちょうど 1 回呼ぶ。要求を処理できなかったときも、エラー応答のフレーム(large-data.md §3.2 の `kind: error`)を作って送る。
- `handle` の戻り値:
  - `0`: 応答を送った(エラー応答を含む)。
  - `JUIZ_BROKER_FRAME_ERROR`: 要求のフレームが読めなかった。ホストは読めたところまでで id がわかればその id で、わからなければ id 0 でエラー応答を送ろうとする(`len` が上限を超えて読まなかったときも id 0)。クライアントのホストは、id 0 の `kind: error` の応答を id の食い違いにせず、その文のエラーにする。区切りがわからなくなったかもしれないので、ストリームのプラグインはその接続を閉じる。
  - `JUIZ_BROKER_SEND_FAILED`: `sender->send` が負の値を返した(応答を送れなかった)。
- `peer_addr` は相手の住所(`<ip>:<port>`、UTF-8)。わからなければ長さ 0。ホストは `SocketAddr` として読めれば `on_capsule_map_request` の `requesting_host_addr` に渡す(今の qmp と同じ)。
- `server_stop` は、受け取りをやめ、処理中の `handle` がすべて戻るのを待ってから戻らなければならない。戻った後、プラグインは `host` の関数を呼んではならない。ホストは `server_stop` の後に `host` の `ctx` を捨てる。
- 1 つのプラグインで `server_start` を何度呼んでもよい(conf の `brokers` に同じ種類を 2 つ書いたとき)。`handle` ごとに独立でなければならない。

### 5.4 クライアント

```text
ホスト                                       プラグイン
  | client_connect(manifest) -------------->  つなぐ(遅らせてもよい)
  | <------------------------------------ 0 と handle
  | client_request(handle, source, receiver)
  |                                           source->write_to(書き口)で要求を書く(§5.2)、送る
  |                                           応答を待つ
  | <--- receiver->receive(reader)
  |  reader から応答を読む(§5.1)
  | ---- receive が 0 を返す ------------->
  | <------------------------------------ client_request が 0 を返す
  | client_close(handle) ------------------>  切る
```

- `client_connect` は manifest(conf の `broker_proxies` の項目、または ID から作った `{"type_name": …, "name": <host:port>}`)を受け取る。接続を開くのは最初の `client_request` まで遅らせてよい。
- `client_request` は応答を受け取り終えるまで戻らない(ブロックする)。ホストは非同期のランタイムのワーカーの中からは呼ばない(`spawn_blocking` などで呼ぶ)。プラグインの中の tokio はホストの tokio とは別のもの(cdylib に静的にリンクされる)なので、今の Zenoh のプロキシのように「ホストのランタイムの中では `wait()` を使えない」ことは起きない。
- **同じ `handle` で、同時にいくつ呼んでもよい**(`BrokerProxy` は `RwLock` の読みで呼ばれる、#362)。プラグインは `handle` をスレッドをまたいで安全に使えるようにしなければならない。
- プラグインは要求を送った後、応答が来たら `receiver->receive` をちょうど 1 回呼ぶ。来なければ呼ばずにエラーを返す。
- 待ち時間はプラグインが決める(manifest の `timeout_ms`、既定は種類ごと)。過ぎたら `JUIZ_BROKER_TIMEOUT` を返す。
- 応答の id が要求の id と同じか、`kind: error` の応答を `BrokerProxyRemoteError` にすることは、ホストがする(今の qmp・Zenoh のプロキシと同じ)。
- `client_close` は、その `handle` の `client_request` がすべて戻った後にだけ呼ぶ(ホストが保証する)。戻った後、`handle` は使わない。

### 5.5 manifest

- manifest は conf の項目の JSON そのもの(ホストが `{target}` の置き換えなどを済ませたもの)。ホストは `type_name`・`name` を足す。種類ごとのキー(`zenoh_config`・`shared_memory` など)はプラグインが読む。
- ホストも読むキー: `name`(§5.3)、`max_frame_bytes`(読み取りの上限、large-data.md §3.5)。プラグインも同じ値を自分の通り道の上限に使ってよい(Zenoh の `transport/link/rx/max_message_size` など)。
- プラグインは知らないキーを無視してよい。

## 6. ホストの共通の処理(Step 1)

### 6.1 サーバー

juiz_core に、C ABI のブローカーのための `BrokerFactory` を 1 つ置く(`juiz_core::cabi::CabiBrokerFactory`)。種類ごとの違いは入口の表だけで、作るのは `CRUDBroker` を持つ `CabiBroker`(`CRUDBrokerHolder` と同じく `CRUDBroker` を持つが、tokio のタスクは使わない)。

- `start` は別のスレッドで `server_start` を呼び、戻ったら名前を直して(`0.0.0.0`・`::` はホストでも `127.0.0.1` に直す)起動済みにする。失敗したら `set_start_failed` に理由を入れ、`wait_until_started` が待たずにその理由を返す。`stop`(と `Drop`)は `server_stop` を呼ぶ。
- `handle` の中身: 読み口からフレームを読む → `IncomingFrame::into_capsule_map` → `localize_request`(§6.3)→ `CRUDBroker::dispatcher()` の `on_capsule_map_request`(ロックは名前と dispatcher を取り出す間だけ、#356)→ `OutgoingFrame::response` か `OutgoingFrame::error` → `sender->send`。今の Zenoh の `respond` と同じ流れ。

### 6.2 クライアント

juiz_core に、C ABI のブローカーのための `CRUDBrokerProxy` の実装を 1 つ置く(`CabiBrokerProxy`)。今の Zenoh の `ZenohBrokerProxy::crud_request` と同じく、`method_name`・`class_name`・`function_name`・`accessed_broker_name` を param に入れ、id を振って `OutgoingFrame::request` を作り、`client_request` で送り、応答を読んで `into_capsule_ptr` にする。`CRUDBrokerProxyHolder` で包むので、`BrokerProxy` の各メソッドは今の qmp・Zenoh と同じになる。

- `topic_push_shared`(IPC の共有メモリの Topic、large-data.md §6)は使えない(既定の実装のエラーのまま)。

### 6.3 自分宛ての ID の書き換え

今 qmp・Zenoh・IPC がそれぞれ持っている `localize_request` を、`type_name` を引数に取る 1 つの関数にしてホストに置く(`juiz_core::brokers::localize_request`。qmp・Zenoh・IPC を載せ替えるのはそれぞれの Step で)。中身は今の Zenoh のものと同じ(`accessed_broker_name` が無ければ自分の名前、`identifier` を `localize_identifier` で 2 回、`accessed_broker_id`・`accessed_broker_type`・`accessed_broker_name` を入れる)。プラグインは書き換えをしない。

### 6.4 種類の決まり方

ホストは、ある `type_name` のブローカーを次の順で探し、最初に見つかったものを使う。

1. conf の `plugins.broker_factories` で読み込んだプラグイン(C ABI でも Rust ABI でも、§11)
2. 組み込み(`register_builtin_broker_factory`、juiz_app の qmp・feature の Zenoh など。C ABI のブローカーを rlib としてリンクしたものは `register_builtin_cabi_broker(<クレート>::broker_factory_cabi)` で登録する)
3. 置き場所(§10)にあるもの

- 1 が 2 より先なのは今と同じ(conf でプラグインを書いたら組み込みより優先する、builtin_broker_factories.rs)。
- 3 は、conf の `brokers`・`broker_proxies`・`mastersystem.broker_type`、または ID の頭(`zenoh://…`)で、1・2 に無い種類が要るようになったときにだけ見る(System を作るときに置き場所のすべてを読み込みはしない)。一度読み込んだら、その System の種類として登録する。
- どこにも無ければ、`BrokerFactoryNotFound` 系のエラーに次の案内を付ける(今の `broker_factory_hint` を置き換える、Step 3)。
  - §10.3 の名前の表にある種類: `The broker type 'zenoh' is not installed. Install it with 'juiz broker install zenoh'.`
  - 無い種類: `The broker type '<名前>' is not built in or installed. Load its plugin with plugins.broker_factories in the conf, or install it with 'juiz broker install <名前> --from <URL>'.`

## 7. エラーと panic

### 7.1 エラーコード

入口の関数とホストの用意する関数の戻り値。`juiz_sdk.h` の既存のコード(-101〜-1104。C++ の -10xx・Zig の -11xx を含む)と rust-plugin-abi.md の -8xx と重ならないよう、-12xx を使う。

| 値 | 名前 | 意味 | エラー文 |
|---|---|---|---|
| `0` | `JUIZ_OK` | 成功 | 書かない |
| `-1201` | `JUIZ_BROKER_ERROR` | プラグインの失敗(待ち受け・接続・送信に失敗した、manifest が合わない) | 何に失敗したか |
| `-1202` | `JUIZ_BROKER_PANIC` | 入口の中で panic した | `panicked: <payload>`(rust-plugin-abi.md §6.1 と同じ) |
| `-1203` | `JUIZ_BROKER_FRAME_ERROR` | フレームが読めない・書けない(§5.3 の `handle`、上限を超える) | 何が合わないか |
| `-1204` | `JUIZ_BROKER_HOST_REFUSED` | ホストの `write_to`・`receive`・`handle` が負の値を返した | 書かない(ホストは理由を知っている) |
| `-1205` | `JUIZ_BROKER_NULL_ARGUMENT` | 必要なポインタが null だった | 書かない |
| `-1206` | `JUIZ_BROKER_TIMEOUT` | 待ち時間のうちに応答が来なかった | 待った時間 |
| `-1207` | `JUIZ_BROKER_UNAVAILABLE` | 相手につながらない・接続が切れた | 相手の住所と理由 |
| `-1208` | `JUIZ_BROKER_SEND_FAILED` | プラグインの `write`・`send`・`read` が負の値を返した(ホストが返す) | 書かない |

- 戻り値が負のとき、プラグインは `error` の sink(`info` では `out`)に高々 1 回エラー文を書く。書かなくてもよい。
- ホストは、プラグインの `JUIZ_BROKER_HOST_REFUSED` を受けたら、自分が断った理由(フレームが読めない、上限を超えるなど)をエラーの文にする。
- 知らない負の値は、ホストはそのままエラーの文に数字を入れる。
- ホストは、クライアントのエラーを今のプロキシのエラーと同じ道筋で呼び出し元に返す。文は `<type_name>://<name>: <エラー文>`(今の Zenoh と同じ形)にする。`JUIZ_BROKER_TIMEOUT` は `no response within <ms> ms` を含める(今の文を保つ)。

### 7.2 panic

- グルーは入口の関数(`info`・`init`・`server_start`・`server_stop`・`client_connect`・`client_request`・`client_close`)と、プラグインが用意する読み口・書き口・送り先の関数の中身をすべて `catch_unwind` で囲む(rust-plugin-abi.md §6.2 と同じ理由)。`server_stop`・`client_close` は戻り値を持たないので、panic したらログに出して握りつぶす。
- ホストは自分の用意する関数(`handle`・`write_to`・`receive`・`log`)の中身を `catch_unwind` で囲む。`handle` の中の panic は、エラー応答(`panicked: …`)を送ってから `0` を返す(Process の実行の panic を相手に伝えるため)。送れなければ `JUIZ_BROKER_SEND_FAILED`。
- プラグインが自分で作ったスレッドの panic は、プラグインが扱う。

### 7.3 ホストのエラー

juiz_sdk の `JuizError` に足す(Step 1)。

```rust
#[error("broker plugin ({plugin}) failed in '{symbol}': {message}")]
BrokerPluginCallError { plugin: String, symbol: String, message: String },

#[error("broker plugin ({plugin}) uses broker plugin ABI {plugin_abi}, but this juiz supports ABI {host_abi}. {hint}")]
BrokerPluginAbiVersionError { plugin: String, plugin_abi: u32, host_abi: String, hint: String },

#[error("broker plugin ({plugin}) can not be loaded. {message}")]
BrokerPluginLoadError { plugin: String, message: String },
```

- `plugin` はプラグインのパス、`symbol` は `broker_factory_cabi.<関数名>`(例 `broker_factory_cabi.server_start`)。
- `hint` は rust-plugin-abi.md §6.3 と同じ(新しければ `Update juiz.`、古ければ `Install a newer version of the broker (juiz broker install <名前> --force).`)。
- `BrokerPluginLoadError` は、表の関数が null、`info` が読めない・必須のキーが無い・`abi_version` が表と違う、`init` が失敗した、のとき。

## 8. プラグイン側のグルー(juiz_sdk)

Step 1 で juiz_sdk の `juiz_sdk::broker_cabi` に置く。プラグインを書く人は Rust の trait を実装し、マクロ 1 つで入口を作る。

```rust
use juiz_sdk::broker_cabi::{self, BrokerPlugin, ServerHandle, ClientHandle, ServerHost,
                            FrameSource, FrameReceiver, BrokerError};

struct MyBroker;

impl BrokerPlugin for MyBroker {
    const TYPE_NAME: &'static str = "mytcp";
    const DEFAULT_PORT: Option<u16> = Some(7500);
    type Server = MyServer;
    type Client = MyClient;

    /// 待ち受けを始めて、実際の住所(name)と一緒に返す
    fn start(manifest: serde_json::Value, host: ServerHost) -> Result<(Self::Server, String), BrokerError>;
    fn connect(manifest: serde_json::Value) -> Result<Self::Client, BrokerError>;
}

impl ServerHandle for MyServer {           // Send + Sync
    fn stop(self);                          // 処理中の host.handle(...) が戻るまで待つ
}

impl ClientHandle for MyClient {           // Send + Sync
    fn request(&self, request: FrameSource<'_>, response: FrameReceiver<'_>) -> Result<(), BrokerError>;
}

juiz_sdk::export_broker_plugin!(MyBroker);  // broker_factory_cabi と表・info・init を作る
```

- `ServerHost` は `Clone + Send + Sync`。`host.handle(peer_addr, reader, sender)` の `reader` は `std::io::Read` を実装した値と長さ、`sender` は `FnOnce(FrameSource<'_>) -> Result<(), BrokerError>`。グルーがこれを §4 の C の構造体に包む。
- `FrameSource` は `len()` と `write_to(&mut impl std::io::Write)`。共有メモリへは、確保した領域を指す `&mut [u8]` を `Write` にして渡せばよい(`write_to` は合計 `len()` バイトをちょうど書く)。
- `FrameReceiver::receive(reader: &mut impl std::io::Read, len: Option<u64>)`。Zenoh なら `ZBytes::reader()` をそのまま渡す。
- `BrokerError` は §7.1 のコード(`Error`・`Timeout`・`Unavailable`・`Frame`)と文を持つ。
- グルーは juiz_core に依存しない。フレームを組み立ても読みもしないので、`juiz_sdk::frame` にも依存しない。
- `info` の `build_info` は `juiz_sdk::build_info`、`plugin_name`・`plugin_version` はマクロが `env!("CARGO_PKG_NAME")`・`env!("CARGO_PKG_VERSION")` から入れる。
- ホスト側の C の受け渡し(表を読んで確かめる `BrokerEntry::load`、読み口・書き口を `Read` / `Write` にした `PluginReader` / `PluginWriter`、サーバーの `RunningServer`、クライアントの `Client`、ログの口)も `juiz_sdk::broker_cabi::host` に置き、juiz_core はその上でフレームの処理だけを書く。同じモジュールに、受け取った要求をそのまま返す `EchoHandler` を置き、juiz_core なしでプラグインのサーバーとクライアントをつないで確かめられるようにする(ループバック)。juiz_core の broker_suite(brokers/broker_suite、#522)は C ABI のブローカーにもそのまま使う(ホストの共通の処理を通るので)。
- 試験用のブローカーは `brokers/tcp_test_broker`(クレート `juiz_tcp_test_broker`、種類名 `tcptest`、公開しない)。グルーだけで作り、長さ(u64 LE)を前に付けたフレームを TCP で送る。これで broker_suite と C ABI だけの確認(待ち受けの失敗、待ち時間切れ、上限、panic、HTTP の処理の中からの呼び出し、`abi: rust`)を走らせる。

C 以外の言語(C++ など)でブローカーを書くことも、この表の形なら原理上はできる。ヘッダ(`juiz_broker.h`)を配るのは今回はしない(§13)。

## 9. ABI の版

### 9.1 版を上げるとき

rust-plugin-abi.md §9.1 と同じ。上げる: 表の既存の項目の並び・型・意味、§4 の構造体、§5 の必須の規則、エラーコードの意味を変える。上げない: 表・構造体の後ろに項目を足す(`struct_size`)、`info` に任意のキーを足す、エラーコードを足す。

### 9.2 ホストの検査

- ホストは自分が読めるブローカーの ABI の版の集合を持つ。最初は `{1}`。
- 読み込むとき、表の `abi_version` が集合に無ければ `BrokerPluginAbiVersionError` にする。`info` の `abi_version` が表と違えば `BrokerPluginLoadError`。
- rustc・juiz_sdk の版は比べない。`build_info` をログ(debug)に出す。
- `juiz broker install`・`juiz broker list`・`juiz pack` は、プラグインを読み込まずに §9.3 の印で版を知る。

### 9.3 ファイルの中の印

- 表の `abi_marker` は `"juiz_broker_abi=<版>;"` の C 文字列で、表から参照されるのでリンカに消されない。rust-plugin-abi.md §9.3 の `juiz_rust_abi=` と同じやり方で、ファイルのバイト列から探す。別ターゲットのバイナリでも読める。

## 10. 置き場所と `juiz broker` コマンド(Step 3)

### 10.1 置き場所

```text
~/.juiz/brokers/                       JUIZ_BROKER_HOME で変えられる(Windows は %USERPROFILE%\.juiz\brokers)
  zenoh/                               種類名(= info の type_name)
    0.1.0/                             プラグインの版
      juiz-broker.json
      lib/libjuiz_zenoh_broker.so      (macOS は .dylib、Windows は juiz_zenoh_broker.dll)
      LICENSE-MIT, LICENSE-APACHE, …
    0.2.0/
      …
```

`juiz-broker.json`(アーカイブに入れる。`juiz broker install` は照合し、読み込みはこれでライブラリを探す):

```json
{
  "format": 1,
  "type_name": "zenoh",
  "plugin_name": "juiz_zenoh_broker",
  "version": "0.1.0",
  "abi_version": 1,
  "target": "x86_64-unknown-linux-gnu",
  "library": "lib/libjuiz_zenoh_broker.so"
}
```

- `type_name`・`version` はディレクトリの名前と同じでなければならない。違うディレクトリは読み込みで無視し、`juiz broker list` で `invalid` と出す。
- 読み込むときに使う版: その種類のディレクトリの中で、`abi_version` がホストの集合にあり、`target` がホストのターゲットと同じものの、semver でいちばん新しいもの。
- 版を固定したいときは、conf の `plugins.broker_factories` にそのライブラリの `path` を書く(§6.4 の 1)。`juiz broker path zenoh --version 0.1.0` で場所がわかる。
- ターゲット名は `juiz sdk install` の 5 種(`x86_64-unknown-linux-gnu`・`aarch64-unknown-linux-gnu`・`x86_64-apple-darwin`・`aarch64-apple-darwin`・`x86_64-pc-windows-msvc`)。

### 10.2 アーカイブ

- 名前: `juiz_broker-<type_name>-<版>-abi<ABI の版>-<ターゲット>.tar.gz`(Windows は `.zip`)。中身は §10.1 の `<版>/` の下と同じ並び(先頭にディレクトリを置かない)。
- 隣に `<アーカイブ名>.sha256`(`sha256sum` の形、`<16 進> <ファイル名>`)を置く。
- 置く先は GitHub Releases の `v<版>` のリリース(Step 4 で juiz_zenoh_broker の公開ワークフローに入れる)。

### 10.3 名前の表

juiz は、種類名から取得先のリポジトリへの小さな表を持つ。最初は 1 行だけ。

| 名前 | 取得先 |
|---|---|
| `zenoh` | `https://github.com/OpenJUIZ/juiz_zenoh_broker` |

表に無い種類は `--from` で入れる。第三者のブローカーの登録先(表を外に置くか)は後で決める(§13)。

### 10.4 コマンド

```text
juiz broker install <名前> [--version <版>] [--from <URL|file://パス>] [--force]
juiz broker list
juiz broker path <名前> [--version <版>]
juiz broker remove <名前> [--version <版>]
```

**install**

1. 取得するアーカイブを決める。
   - `--from` があれば、その URL・ファイルのアーカイブ。`https://` なら `<URL>.sha256` が無ければエラー。`file://` は `.sha256` があれば照合し、無ければ警告を出して続ける。
   - `--version` があれば、名前の表のリポジトリの `releases/download/v<版>/` から、ホストの集合の ABI の版を大きい順に試し、最初に見つかった `abi<N>` のアーカイブ。
   - どちらも無ければ、GitHub の REST API(`GET /repos/<owner>/<repo>/releases`、`GITHUB_TOKEN` があれば付ける)でリリースを新しい順に見て、下書き・プレリリースでなく、ホストのターゲットとホストの集合の ABI の版のアーカイブを持つ最初のもの。ABI の版が合わない新しいリリースを飛ばしたら、`note: juiz_zenoh_broker 0.3.0 needs broker ABI 2; this juiz supports ABI 1. Installing 0.2.1.` を出す。
   - 取得元の頭は `JUIZ_BROKER_BASE_URL` で変えられる(`file://<ディレクトリ>` なら `<ディレクトリ>/v<版>/<アーカイブ>`、版を省いたときは `v*` のディレクトリを API の代わりに見る。テスト用)。
2. `.sha256` と照合する。違えばエラーにして何も残さない。
3. 置き場所の中の一時ディレクトリに展開し、`juiz-broker.json` を確かめる(`type_name` が `<名前>` と同じ、`abi_version` がホストの集合にある、`target` がホストと同じ、`library` があり、その中に `juiz_broker_abi=<abi_version>;` の印がある)。合わなければエラー。
4. `<名前>/<版>/` へ名前を変えて移す。すでにあれば `already installed (use --force to install it again)` と出して何もしない。`--force` なら入れ替える。
5. `installed zenoh broker 0.1.0 (juiz_zenoh_broker, broker ABI 1) to <パス>` と、使い方の 1 行(`Use it with 'type_name: zenoh' in the conf, or IDs like zenoh://<host>:<port>/…`)を出す。

**list** は、組み込みの種類(`http`・`ipc`・`qmp`、feature の `zenoh` など、`builtin` と出す)と置き場所の全部を並べる。置き場所のものは版・ABI の版・使われるか(`used` / `older` / `ABI 2 is not supported by this juiz` / `invalid`)を出す。

**path** は、その種類で読み込みに使われる版(§10.1)のディレクトリを出す。`--version` ならその版。無ければエラー(終了コード 1)。

**remove** は、`--version` があればその版を、無ければその種類の全部の版を消し、消したディレクトリを出す。

## 11. Rust ABI のブローカーとの並存

- ホストは `plugins.broker_factories` の各プラグインについて、まず `broker_factory_cabi` を探す。あれば C ABI で読み、`broker_factory_build_info` の検査(#450)はしない。
- 無ければ今の Rust ABI の `broker_factory` / `broker_proxy_factory` を、今の検査付きで読む。
- conf のプラグインの項目に `abi: rust` を書くと、`broker_factory_cabi` があっても Rust ABI で読む。`abi: c`(または書かない)は上の既定、それ以外の値は `InvalidSettingError`(rust-plugin-abi.md §11 と同じ)。
- 置き場所(§10)からは C ABI だけを読む。
- 組み込み(qmp・HTTP・IPC)は今までどおり juiz_core の中の Rust のまま。C ABI にするかは後で決める(§13)。
- Rust ABI のブローカーの読み込みをいつやめるかは後で決める(別 issue)。

## 12. .juizapp への同梱

- C ABI のブローカーのプラグインは、今のブローカーのプラグインと同じく `plugins/<target>/` に置き、entry の conf の `plugins.broker_factories` に `path`(`{target}` を含む)で書く。
- `juiz pack` の段 6(app-package.md §6.3)は、`broker_factories` のプラグインの印を見て分ける。`juiz_broker_abi=<版>;` があれば C ABI のブローカーで、rustc・juiz_sdk の検査をしない(Rust ABI のプラグインとして数えない)。版を manifest の `broker_abi` に書く(すべて同じでなければエラー)。印が無ければ今までどおり Rust ABI。
- `juiz run` の段 6(app-package.md §9)は、`broker_abi` があればホストがその版を読めるかを確かめ、読めなければ `<name> <ver> contains broker plugins with broker ABI <n>, but this juiz supports ABI <版の並び>. Update juiz.` で止める。
- パッケージに同梱せず、置き場所のものを使ってもらうこともできる(conf の `type_name: zenoh` だけを書く)。そのとき受け取った人の juiz に無ければ、§6.4 の案内(`juiz broker install zenoh`)が出る。
- 実装は Step 3(#638)でする。

## 13. 範囲外(後で決めること)

- qmp・IPC・HTTP を C ABI のプラグインに移すか。今回は組み込みのまま
- サーバーから相手へ一方的に送る形(Topic の put / subscribe など)。今は要求・応答だけ。入れるときは表の後ろに項目を足す(版は上げない、§9.1)
- 第三者のブローカーの登録先(名前の表を juiz の外に置くか、§10.3)
- C・C++ 向けのヘッダ `juiz_broker.h` を配ること
- プラグインのブローカーに、受け手の最終的な領域を共有メモリのまま使わせること(受け手での写しを無くす。zenoh-broker.md §11.5 の残りの時間)。画像のカプセルが共有メモリを指せる必要があり、ここではしない
- 実行中にプラグインを外すこと(外さない前提。`Arc<Library>` をプロセスが終わるまで持つ)
- Rust ABI のブローカーの読み込みをやめる版(§11)

## 14. Step との対応

| Step | issue | この文書の節 |
|---|---|---|
| 0 仕様書 | #635 | 全体、rust-plugin-abi.md §12・§13 |
| 1 ホストと juiz_sdk のグルー | #636 | §3〜§9、§11。試験用の小さなブローカー(TCP など)で broker_suite を通す |
| 2 Zenoh を C ABI に載せ替え | #637 | §5.1・§5.2 の共有メモリでの使い方。broker_suite と共有メモリのテスト。完了の条件は zenoh-broker.md §11.5 のベンチ(720p・4K、TCP と共有メモリ)との差が誤差の範囲であること |
| 3 `juiz broker` と置き場所 | #638 | §6.4、§10、§12 |
| 4 juiz_zenoh_broker へ移す | #639 | §10.2 のアーカイブと公開ワークフロー |
| 5 juiz から Zenoh を外す | #640 | §6.4 の案内、zenoh-broker.md §8 |
| 6 サイト | site#127 | `juiz broker` のガイド、ブローカーの作り方(§8) |
| 7 リリース | #641 | |

## 15. 計画から変えたこと・細かく決めたこと

Step 2(#637、Zenoh の載せ替え)で決めたこと:

- **入口の表を `#[no_mangle]` なしで作るマクロ `define_broker_entry!` を足した。** `export_broker_plugin!` は中でこれを使い、`broker_factory_cabi` から呼ぶ。組み込み用のクレート(juiz_app に rlib としてリンクする Zenoh など)は `define_broker_entry!(T, pub fn broker_entry)` で作った関数を `register_builtin_cabi_broker` に渡し、`broker_factory_cabi` は feature(`plugin`)のときだけ置く。C ABI のブローカーを 2 つ以上 juiz に組み込んでもリンクで名前がぶつからない。
- **Zenoh は Rust ABI の入口を並べて置かない。** §3.1 では移行のあいだ並べてよいとしたが、置くと juiz_core に依存し続けることになるので外した。`abi: rust` で Zenoh は読めない。
- ベンチ(TCP・共有メモリ、720p・4K)は載せ替えの前後で揺れの範囲だった(zenoh-broker.md §15)。

Step 1(#636)の実装で決めたこと:

- **ホスト側の C の受け渡しも juiz_sdk に置いた。** §8 では `juiz_sdk::broker_cabi::host` を試験用の道具としていたが、juiz_core のホストもこれを使う。unsafe な受け渡しを 1 か所にまとめ、グルーと同じクレートで両側を試せるようにするため。juiz_core に残るのはフレームの処理・`localize_request`・`CRUDBroker` への振り分けだけ。
- **サーバーは `CRUDBrokerHolder` でなく `CabiBroker`。** `CRUDBrokerHolder` は種類名が `&'static str`・起動の関数が `Copy` の関数で、読み込んだ表を持てないため(§6.1)。
- **id 0 のエラー応答はその文のエラーにする。** 上限を超えて読まなかった要求には id がわからないため(§5.3)。
- 今の System は同じ種類のブローカーを 1 つしか持たない(conf の `brokers` に同じ種類を 2 つ書くと後のものだけが立つ)。§5.3 の「`server_start` を何度呼んでもよい」はプラグインの側の約束として残す。

- **入口はサーバーとクライアントを 1 つの表にした。** 計画の `start` / `stop` / `connect` / `request` / `close` を、rust-plugin-abi.md と同じく `broker_factory_cabi` が返す 1 つの表にまとめた。今の `broker_factory` / `broker_proxy_factory` の 2 つに分かれていたものも 1 つになる。
- **フレームは「読み口」と「書き口」で受け渡す。** 計画の `request(handle, frame) -> frame` のようにバイト列を丸ごと渡すと、受け取りで「通り道 → プラグインのバッファ → ホストの最終的な領域」と 1 回増える。そこで、受け取りはホストが最終的な領域を `read` に渡して直接書かせ(#339 と同じ考え)、送り出しはプラグインが確保した領域(共有メモリ)へホストが `write` で直接書くことにした。どちらも今の qmp・Zenoh の写しの回数と同じ。
- **プラグインはフレームの形を知らない。** ストリームの区切りはプラグインが自分の長さで付ける(§5.1)。JZF1 の読み書きはホストだけが持つので、フレームの形を変えてもプラグインを作り直さなくてよい。グルーも `juiz_sdk::frame` に依存しない。
- **待ち受けの失敗はエラーで返す。** 今の qmp・Zenoh の `on_start` は log に出して終わるだけで、`start_brokers` が待ち時間切れになって原因が見えなかった。`server_start` は失敗の理由を返す(§5.3)。
- **名前の付け直しと起動済みにするのはホスト。** 今は各ブローカーの `on_start` が `update_broker_name` と `set_started` をしていた。`server_start` が実際の住所を返し、ホストがする(§5.3)。
- **ログの口を最初から入れた。** Process の C ABI(rust-plugin-abi.md §13)では範囲外にしたが、ブローカーは自分のスレッドのエラーがログにしか出ないので入れた(§3.4)。
- **探す順は「conf → 組み込み → 置き場所」。** 計画では「組み込み → conf → 置き場所」と書いたが、今の juiz は conf のプラグインを組み込みより優先している(builtin_broker_factories.rs)ので、それを保った(§6.4)。置き場所は、ほかに無いときにだけ見る。
- **アーカイブ名に ABI の版を入れた。** 版を省いた `install` が、アーカイブを落とさずにリリースの一覧だけで「この juiz で読める最新」を選べるようにするため(§10.2・§10.4)。
- **エラーコードは -12xx。** `juiz_sdk.h` が -9xx を Transformations に、-10xx を C++ の例外に、-11xx を Zig に使っているため(§7.1)。
