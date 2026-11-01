# Rust プラグインの C ABI 仕様

- 状態: 確定(Rust プラグイン C ABI 化 Step 0、issue #328、2026-10-31)
- 親 issue: #291。計画と経緯はプロジェクトの計画書 rust-c-abi-plan.md(2026-10-31 確定、決めごと A〜C はおすすめ通り)にある
- 関連: フレーム形式 [large-data.md](large-data.md) §3、遅延評価 [lazy-evaluation.md](lazy-evaluation.md) §8〜§10、データ型 [datatype.md](datatype.md) §12・§17、アプリ配布形式 [app-package.md](app-package.md) §4・§6.3・§9・§12.1
- この文書は #329〜#333 の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と用語

Rust のプラグインを、ホスト(juiz)と違う rustc・違う依存クレートの版でビルドしても読み込めるようにする。そのために、ホストとプラグインの間で受け渡すものを **バイト列と C の関数だけ** にする。

今は入口が `extern "Rust"` で、`ProcessFactoryStruct`(Rust の fn ポインタ)、`Arc<dyn Fn>`、`JuizResult`、`CapsuleMap` / `Capsule` をそのまま渡している。これらの配置は rustc の版でも依存クレート(serde_json・image など)の版や feature でも変わりうるので、作った人と同じ rustc でビルドした juiz でしか安全に動かない(`BuildInfo` で rustc と juiz_sdk の版を比べて弾いている)。

| 用語 | 意味 |
|---|---|
| C ABI のプラグイン | この文書の入口(`<symbol>_cabi`、§3)を持つ Rust のプラグイン |
| Rust ABI のプラグイン | 今の `extern "Rust"` の入口だけを持つ Rust のプラグイン。EC・ブローカーのプラグインと、この文書より前の juiz_sdk でビルドしたプラグイン |
| 入口の表 | `<symbol>_cabi()` が返す `#[repr(C)]` の構造体(§3.2)。版と C の関数ポインタを持つ |
| ホスト | プラグインを読み込む juiz(juiz_core)。ホスト自身も Rust だが、プラグインとは別の rustc・juiz_sdk でビルドされていてよい |
| グルー | juiz_sdk の中にある、プラグイン側でフレームを `CapsuleMap` / `Capsule` に組み立て直し、今までどおりの Rust の関数を呼ぶ部分(§7)。juiz_macro が生成する入口から呼ぶ |
| ABI の版 | この文書の入口の表と関数の形の版。最初は `1`(§9) |
| コンテナの型タグ | コンテナの中身の配置が同じかを比べる文字列(§8.2) |
| ネイティブの ABI タグ | 遅延評価の型タグの `abi`(datatype.md §17.1 の `rust_abi_tag()`)。同じプラグイン(cdylib)の中でだけ一致する |

## 2. 基本方針

1. **境界はバイト列と C の関数だけ。** juiz_macro が各ファクトリについて `extern "C"` の入口を生成する。構造体は `#[repr(C)]` の小さいもの(`{ptr, len}` のバイト列、その配列、関数ポインタの表)だけにする。Rust の型(`String`・`Vec`・`Box<dyn …>`・`JuizResult`・`Capsule`)は境界を越えない。
2. **値は JZF1 フレーム(large-data.md §3)で運ぶ。** ヘッダの JSON と本体を、それぞれポインタで渡す。1 つのバッファにまとめて複製はしない。画像のコピーは行き(ホスト → プラグイン)と帰り(プラグイン → ホスト)で 1 回ずつ。
3. **プラグインの中は今までどおり。** グルーがプラグイン自身の juiz_sdk で `CapsuleMap` を作り、利用者の書いた関数(`fn(CapsuleMap) -> JuizResult<Capsule>` など)を呼ぶ。利用者のコードは書き換えなくてよい。
4. **ホストは ABI の版の数字だけを比べる。** rustc と juiz_sdk の版は比べない(ログに出すだけ)。
5. **panic は境界を越えない。** プラグイン側の入口はすべて `catch_unwind` で囲み、エラーコードにして返す。
6. **対象は Process・Container・ContainerProcess・Component。** EC とブローカーは juiz_core の trait を直接渡しているので Rust ABI のまま残す(§12)。
7. **古いプラグインも当面読める。** `_cabi` の入口が無いプラグインは、今の Rust ABI と `BuildInfo` の検査で読み込む(§11、決めごと A)。

## 3. 入口

### 3.1 シンボルの名前

juiz_macro は、今のファクトリのシンボル `<symbol>` ごとに、`<symbol>_cabi` を生成する。今のシンボルの決め方は変えない。

| マクロ | 今のシンボル(Rust ABI) | 足すシンボル(C ABI) | 表の `kind` |
|---|---|---|---|
| `#[juiz_process]` | `process_factory` | `process_factory_cabi` | `1` Process |
| `#[juiz_container]` | `container_factory` | `container_factory_cabi` | `2` Container |
| `#[juiz_container_process]` | `container_process_factory` | `container_process_factory_cabi` | `3` ContainerProcess |
| `#[juiz_component_process]` | `<関数名>_factory` | `<関数名>_factory_cabi` | `1` |
| `#[juiz_component_container]` | `<関数名>_factory` | `<関数名>_factory_cabi` | `2` |
| `#[juiz_component_container_process]` | `<関数名>_factory` | `<関数名>_factory_cabi` | `3` |
| `juiz_component_manifest!` | `component_manifest` | `component_manifest_cabi` | `4` Component |

```rust
#[no_mangle]
pub extern "C" fn process_factory_cabi() -> *const JuizRustEntry;
```

- `<symbol>_cabi` は引数を取らず、プラグインの中の `static` な入口の表(§3.2)へのポインタを返す。表はプラグインを外すまで有効でなければならない。null を返してはならない。
- 移行期間(§11)のあいだ、juiz_macro は今の Rust ABI の入口(`<symbol>` と `<symbol>_build_info`)も生成し続ける。この文書より前の juiz でも、同じ rustc なら今までどおり読めるようにするため。
- `juiz_rust_abi_version()`(§9.2)は juiz_sdk が 1 つだけ持つ。マクロは生成しない(Component では 1 つのクレートに複数のマクロが並ぶので、マクロごとに生成すると名前がぶつかる)。

### 3.2 入口の表

```c
/* プラグイン側が static に置く。並びと型を変えるときは ABI の版を上げる(§9.1) */
typedef struct juiz_rust_entry {
    uint32_t abi_version;   /* この表の ABI の版。今は 1 */
    uint32_t struct_size;   /* sizeof(juiz_rust_entry)。後ろに項目を足すときに使う */
    uint32_t kind;          /* 1 Process / 2 Container / 3 ContainerProcess / 4 Component */
    uint32_t reserved;      /* 0 */
    const char* abi_marker; /* "juiz_rust_abi=1;" (§9.3)。pack がファイルのバイト列から探す */

    /* manifest の JSON を out に書く。全種類 */
    int64_t (*manifest)(const juiz_bytes_sink* out);
    /* プラグインの情報の JSON を out に書く(§3.3)。全種類 */
    int64_t (*info)(const juiz_bytes_sink* out);

    /* kind 1: 呼び出し */
    int64_t (*call)(const juiz_frame_in* args, const juiz_frame_out* out);
    /* kind 2: コンテナの作成と破棄(§8) */
    int64_t (*create)(juiz_bytes manifest_json, const juiz_frame_in* args,
                      void** handle, const juiz_bytes_sink* error);
    void (*destroy)(void* handle);
    /* kind 3: コンテナプロセスの呼び出し(§8) */
    int64_t (*container_call)(void* handle, const juiz_frame_in* args,
                              const juiz_frame_out* out);
} juiz_rust_entry;
```

- その `kind` で使わない関数ポインタは null にする。ホストは `kind` に要る関数が null なら読み込みをエラーにする(`RustPluginCallError`、§6.3)。
- ホストは `abi_version` を最初に読み、知らない版なら残りを読まずにエラーにする(§9.2)。
- `struct_size` が、ホストの知っている大きさより小さければエラー、大きければ知っている部分だけを使う。
- Rust では `#[repr(C)] pub struct JuizRustEntry` として juiz_sdk の `juiz_sdk::cabi` に置く(Step 1)。ホストとプラグインは別々の juiz_sdk のこの型を使うが、`repr(C)` なので配置は同じになる。

### 3.3 `info` の JSON

```json
{
  "build_info": "juiz_sdk=0.0.30;rustc=rustc 1.91.0 (f8297e351 2025-10-28)",
  "native_abi_tag": "rust:rustc 1.91.0 (f8297e351 2025-10-28):0.0.30:0x7f3a5c2e10a8",
  "container_tag": "rust:rustc 1.91.0 (f8297e351 2025-10-28):0.0.30:example_container::ExampleContainer:96:8"
}
```

| キー | 種類 | 意味 |
|---|---|---|
| `build_info` | 全種類 | `juiz_sdk::build_info` の文字列。ホストはログに出すだけで比べない |
| `native_abi_tag` | 1・2・3 | `juiz_sdk::types::rust_abi_tag()`。同じプラグインの中でだけ一致する(datatype.md §17.1)。Native をハンドルのまま渡してよいかに使う(§10) |
| `container_tag` | 2・3 | コンテナの型タグ(§8.2)。2 は作るコンテナの、3 は受け取るコンテナのもの |

知らないキーは無視する。

## 4. 構造体

```c
/* 借りたバイト列。持ち主は渡した側で、呼び出しが終わるまで有効 */
typedef struct juiz_bytes {
    const uint8_t* ptr;
    size_t len;
} juiz_bytes;

/* 小さなバイト列(manifest・info・エラー文)の受け取り口。ホストが用意する */
typedef struct juiz_bytes_sink {
    void* ctx;
    void (*write)(void* ctx, juiz_bytes bytes);   /* 1 回だけ呼ぶ。ホストは複製する */
} juiz_bytes_sink;

/* 引数のフレーム。ホストが用意し、呼び出しが終わるまで有効 */
typedef struct juiz_frame_in {
    juiz_bytes header;                 /* ヘッダの JSON(UTF-8) */
    const juiz_bytes* bodies;          /* 本体の配列。ヘッダの bodies と同じ順 */
    size_t body_count;
    const juiz_rust_native* natives;   /* Native の配列(§10.3)。無ければ NULL, 0 */
    size_t native_count;
} juiz_frame_in;

/* 戻り値の書き込み口。ホストが用意する */
typedef struct juiz_frame_out {
    void* ctx;
    /* ヘッダの JSON を渡す。ホストは読んで、本体ごとに最終的な領域を確保する */
    int64_t (*begin)(void* ctx, juiz_bytes header);
    /* 本体 index の書き込み先を受け取る。長さはヘッダの説明から決まる */
    int64_t (*body)(void* ctx, size_t index, uint8_t** ptr, size_t* len);
    /* Native をハンドルのまま渡す(§10.2)。null なら、プラグインは Native を encode して書く */
    int64_t (*native)(void* ctx, size_t index, juiz_rust_native native);
    /* エラー文(UTF-8)。入口が負の値を返すときに 1 回だけ呼ぶ */
    void (*error)(void* ctx, juiz_bytes message);
} juiz_frame_out;
```

- 整数・ポインタの幅は C と同じ(`size_t` は Rust の `usize`、`int64_t` は `i64`)。
- 構造体の中のポインタは、呼ばれた側が呼び出しの後まで持ってはならない(引数のフレーム、`juiz_bytes`)。持ち続けたいものは複製する。
- 関数ポインタは `extern "C"`。どの関数も panic を外に出してはならない(§6.2)。ホストの用意する `begin` / `body` / `native` / `error` / `write` も同じで、ホストは中で `catch_unwind` するか、panic しない書き方にする。

## 5. フレームの使い方

### 5.1 ヘッダ

large-data.md §3.2 のヘッダの形をそのまま使う。違いは次だけ。

| | IPC・qmp(large-data.md) | この文書 |
|---|---|---|
| 先頭の `"JZF1"` と長さ | あり | なし(`juiz_bytes` の `len` がヘッダと本体の長さ) |
| `id` | 要求ごとの通し番号 | `0` を入れる。受け手は見ない |
| `param` | 必須 | `CapsuleMap` に `__param__` があれば入れる。無ければ省く。呼ぶ関数は入口で決まるので `method_name` などは要らない |
| `kind` | `request` / `response` / `error` | 引数は `request`、戻り値は `response`。エラーはフレームではなく戻り値のコードと `error`(§6) |
| 共有メモリ(`shm`) | B で使う | 使わない。`shm` のある本体はエラー |

引数:

```json
{
  "id": 0,
  "kind": "request",
  "map": {"image": {"body": 0}, "gain": {"value": 1.5}},
  "bodies": [{"kind": "image", "width": 1280, "height": 720, "format": "rgb8", "stride": 3840}]
}
```

戻り値:

```json
{
  "id": 0,
  "kind": "response",
  "result": {"body": 0},
  "option": {"note": "..."},
  "bodies": [{"kind": "image", "width": 1280, "height": 720, "format": "rgb8", "stride": 3840}]
}
```

- カプセルの表し方は large-data.md §3.3(`value` / `empty` / `body`)に、`{"native": <番号>}`(§10)を足す。番号は、引数では `juiz_frame_in` の `natives` の添字、戻り値では `native` で渡した順の番号。
- 本体の種類と規則は large-data.md §3.4(`image` / `array`)と同じ。数の配列を本体に出す規則(要素 1024 以上で、整数だけか小数だけ)も同じで、出すかどうかは送り手が決めてよい。受け手は両方を必ず受け付ける。
- 画像の `stride` は `width × 1 画素のバイト数` でなければならない(余白なし)。Rust の送り手は `DynamicImage::as_bytes()` をそのまま渡すので余白は無い。余白があればエラー(受け手で余白を取るともう 1 回コピーが要るため)。
- 戻り値の `option` は `CapsulePtr::set_option` で付けたもの。無ければ省く。引数のオプションは運ばない(IPC 越しと同じ)。
- 値の意味は変えない(large-data.md §2 の 5)。整数と小数の区別、画像の画素形式を保つ。
- 上限は large-data.md §3.5 と同じ(ヘッダ 64 MiB、合計 1 GiB)。

### 5.2 引数(ホスト → プラグイン)

1. ホストは `CapsuleMap` からヘッダを作る。画像のカプセルは本体にし、`juiz_bytes` はホストの `DynamicImage::as_bytes()` をそのまま指す(複製しない)。Native は §10 のとおりにする。
2. ホストは呼び出しのあいだ、引数のカプセルを読みのロックで持ち続ける(本体のポインタを有効に保つため)。
3. プラグインのグルーはヘッダを読み、本体を自分の `DynamicImage` / `Vec` に複製して `CapsuleMap` を作る。**画像のコピーはここで 1 回。**

ホストの実装(Step 2)は juiz_sdk の `OutgoingFrame::with_body_slices` を使う。画像の本体はカプセルをロックしたまま、その画素を直接指すバイト列で渡し、ロックは入口から戻るまで持つ。同じカプセルが 2 つの引数に出てくるときは、最初のロックを使い回す(同じ `Mutex` を 2 度ロックして止まらないため)。

### 5.3 戻り値(プラグイン → ホスト)

1. 利用者の関数が `Capsule` を返したら、グルーはヘッダを作り、`out->begin(ctx, header)` を呼ぶ。
2. ホストはヘッダを読み、本体ごとに最終的な `DynamicImage` / `Vec` の領域を確保する(large-data.md §3.6 の「最終的な領域へ直接読む」と同じ考え)。ヘッダが読めない、上限を超えるときは負の値を返す。
3. グルーは本体ごとに `out->body(ctx, i, &ptr, &len)` を呼び、`len` がヘッダの説明と同じことを確かめ、`ptr` に書く。**画像のコピーはここで 1 回。** `ptr` の領域は 0 で埋めていない(中身は決まっていないか、使い回した前の画像、§5.5)。グルーは `len` バイトを全部書いてから 0 を返す。書かずに 0 を返すことは許さない(Step 6 #339)。
4. グルーが 0 を返したら、ホストは確保した領域から `Capsule` を組み立てる。`begin` を呼ばずに 0 を返したら `Capsule::Empty` とする。
5. プラグインの戻り値の Native は §10.2 のとおりハンドルのまま渡す。ホストが `native` の口を持たない(null)ときは encode してから書く(large-data.md §3.3 と同じ)。

ホストが負の値を返したら、グルーは書くのをやめて `JUIZ_RUST_HOST_REFUSED`(§6.1)を返す(`error` は呼ばない)。

ホストの実装(Step 2)は juiz_sdk の `FrameReceiver` を使う。`begin` でヘッダを読み、上限・`shm`・余白のある `stride` を確かめてから、画素の型ごとの最終的な `Vec<u8>` / `Vec<u16>` / `Vec<f32>`(数の配列は `Vec<i64>` / `Vec<f64>`)を確保し、`body` でその領域を渡す。書き終わったら複製せずに `DynamicImage` / `Value` にする。`JUIZ_RUST_HOST_REFUSED` のときは、ホストが断った理由を `RustPluginCallError` の文に入れる。

`FrameReceiver` は確保した領域を 0 で埋めない(Step 6 #339)。`body` で渡していない本体が残ったまま 0 が返ったら、その本体は書かれていないので、戻り値をエラーにする(`body N was not written`)。

### 5.5 画像の領域の使い回し(Step 6 #339)

大きな画像の領域を呼び出しのたびに確保・解放すると、glibc はその領域を OS に返し、次の確保でページフォールトが起きる(§16、720p で呼び出しの時間の半分以上)。そこで、使い終わった画像の領域を、スレッドごとに取っておいて次の受け取りに使う。large-data.md の IPC の `read_image` の spare と同じ考え。

- **プラグイン側**: グルーは戻り値の画像をホストの領域に書き終えたら、その画像を取っておく。次に引数の画像を読むとき(§5.2 の 3)、同じ幅・高さ・形式のものがあればその領域に読む。
- **ホスト側**: 引数を渡し終えたら(入口から戻ったら)、引数の画像のうちほかから参照されていないもの(呼び手がもう持っていないもの)を取っておく。次に戻り値の画像を受け取るとき(§5.3 の 2)、同じ幅・高さ・形式のものがあればその領域を `body` で渡す。
- 取っておくのは、ほかから参照されていない普通の画像だけ(Native と、呼び手やほかの値がまだ持つ画像は取らない。中身を書き換えてしまうため)。
- 取っておく数はスレッドごとに 2 つまで(`juiz_sdk::frame::SPARE_IMAGES_MAX`)。超えたら古いものから捨てる。そのぶんメモリを使い続ける(4K の RGB なら 1 つ約 25MB)。プラグインごと(juiz_sdk はプラグインごとに静的にリンクされる)とホストに別々にある。
- 境界の形(§4)は変えないので、ABI の版(§9)は 1 のまま。古い(0.0.30 の)グルーのプラグインも、新しいホストで今までどおり動く(ホストの使い回しだけが効く)。

### 5.4 manifest

- `manifest` は今の manifest の関数(`manifest()` / `<関数名>_manifest()` / `component_manifest()`)の結果を `serde_json` で JSON にして `out->write` に渡す。ホストは自分の juiz_sdk の `ProcessManifest` / `ContainerManifest` / `ComponentManifest` に読む。C++ のプラグインの manifest と同じ道筋になる。
- manifest を作るときのエラー(既定値が型に合わないなど、datatype.md §17)は今は panic している。C ABI では `catch_unwind` で捕まえ、`JUIZ_RUST_PANIC` で返す(§6.2)。ホストはプラグインの読み込みを、関数の名前を付けたエラーにする。
- `manifest` と `info` には `error` の口が無いので、負の値を返すときは **同じ `out` にエラー文を書く**(`write` は成功でも失敗でも高々 1 回)。ホストは戻り値が負なら、受け取ったバイト列を JSON ではなくエラー文として読む。`JUIZ_RUST_NULL_ARGUMENT` のときは書かない。

## 6. エラーと panic

### 6.1 エラーコード

入口の関数の戻り値。`juiz_sdk.h` の既存のコード(-101〜-704)と重ならないよう -8xx を使う。

| 値 | 名前 | 意味 | `error` の文 |
|---|---|---|---|
| `0` | `JUIZ_OK` | 成功 | 呼ばない |
| `-801` | `JUIZ_RUST_CALL_ERROR` | 利用者の関数(manifest・作成・呼び出し)が `Err(JuizError)` を返した | `format!("{e:#}")` |
| `-802` | `JUIZ_RUST_PANIC` | 入口の中で panic した | `panicked: <payload>`(payload が `&str` / `String` ならその文、ほかは `<non-string panic payload>`) |
| `-803` | `JUIZ_RUST_FRAME_ERROR` | 引数のフレームが読めない(ヘッダの JSON、本体の数・長さ、`shm`、余白のある `stride`、知らない本体の種類) | 何が合わないか |
| `-804` | `JUIZ_RUST_HOST_REFUSED` | ホストの `begin` / `body` / `native` が負の値を返した | 呼ばない(ホストは理由を知っている) |
| `-805` | `JUIZ_RUST_NULL_ARGUMENT` | 必要なポインタが null だった | 呼ばない |

- 戻り値が負のとき、プラグインは `error`(作成では `error` の sink、`manifest` / `info` では `out`、§5.4)を高々 1 回呼ぶ。呼ばなくてもよい。
- 知らない負の値は、ホストはそのまま `RustPluginCallError` の文に数字を入れる。

### 6.2 panic

- グルーは、入口の関数(`manifest` / `info` / `call` / `create` / `destroy` / `container_call` と Native の表の関数)の中身をすべて `std::panic::catch_unwind` で囲まなければならない。`extern "C"` の関数から panic を外へ出すと、Rust 1.81 からはその場でプロセスが止まり、それより前は未定義動作になる。今の Rust ABI の入口は、panic がホストまで巻き戻る。
- `destroy` は戻り値を持たないので、panic したら握りつぶしてログに出す。
- `panic = "abort"` でビルドしたプラグインの panic は捕まえられない(プロセスが止まる)。これは利用者の選択とし、juiz は対処しない。
- panic した後の利用者のデータ(コンテナの中身など)は壊れているかもしれない。コンテナを毒にする(以後の呼び出しをエラーにする)かは決めない。今の Rust ABI のプラグインと同じく、続けて使える。

### 6.3 ホストのエラー

juiz_sdk の `JuizError` に足す。

```rust
#[error("Rust plugin ({plugin}) failed in '{symbol}': {message}")]
RustPluginCallError { plugin: String, symbol: String, message: String },

#[error("Rust plugin ({plugin}) uses Rust plugin ABI {plugin_abi} in '{symbol}', but this juiz supports ABI {host_abi}. {hint}")]
RustPluginAbiVersionError { plugin: String, symbol: String, plugin_abi: u32, host_abi: String, hint: String },
```

- `plugin` はプラグインのパス、`symbol` は `<symbol>_cabi.<関数名>`(例: `process_factory_cabi.call`)。
- `message` はプラグインが `error` で渡した文。渡さなかったときは `returned <コード> (<名前>)`。panic なら `panicked: …` で始まる。
- `RustPluginAbiVersionError` の `hint` は、プラグインの版がホストより新しければ `Update juiz.`、古ければ(将来ホストが古い版をやめたとき)`Rebuild the plugin with a newer juiz_sdk.`。
- 呼び出しのエラーは、今の Process の呼び出しのエラーと同じ道筋で呼び出し元に返る(ブローカー越しなら `BrokerProxyRemoteError` の文になる)。

## 7. プラグイン側のグルー(juiz_sdk)

Step 1 で juiz_sdk の `juiz_sdk::cabi` に置く。juiz_macro が生成するのは、表の `static` と、グルーを呼ぶ薄い `extern "C"` 関数だけにする。

```rust
// juiz_macro が #[juiz_process] から生成するもの(形の例)
#[no_mangle]
pub extern "C" fn process_factory_cabi() -> *const juiz_sdk::cabi::JuizRustEntry {
    static ENTRY: juiz_sdk::cabi::JuizRustEntry = juiz_sdk::cabi::JuizRustEntry::process(
        process_factory_cabi_manifest,
        process_factory_cabi_info,
        process_factory_cabi_call,
    );
    &ENTRY
}
extern "C" fn process_factory_cabi_call(args: *const JuizFrameIn, out: *const JuizFrameOut) -> i64 {
    juiz_sdk::cabi::call_process(args, out, my_function)   // catch_unwind・フレームの読み書きはこの中
}
```

- フレームの読み書きは `juiz_sdk::frame` に移した(Step 1。`juiz_core::frame` はその再公開)。ホストのブローカーとグルーで同じものを使う。メモリ上のヘッダと本体から読む `read_frame_parts`(`shm` と余白のある `stride` はエラー)、引数を作る `OutgoingFrame::call_request`(`id` 0、param は空なら省く)、`header_bytes` / `body_len` / `copy_body_into`(書き込み口へ本体を直接写す)、param を要らない `IncomingFrame::into_call_map` を足した。
- グルーの公開関数: `write_manifest`・`process_info`・`container_info`・`container_process_info`・`component_info`・`call_process`・`create_container`・`destroy_container`・`call_container_process`・`container_tag`。コンテナの型 `T` は利用者の関数の型から推論する(マクロは `T` を書かない)。
- `juiz_sdk::cabi::host` に、ホストの代わりにフレームを渡して入口を呼ぶ試験用の道具(`FrameArgs`・`FrameResult`・`collect_bytes`)を置く。引数の本体を複製するので、juiz_core の読み込み(Step 2)には使わない。
- Rust の表では、null になりうる関数ポインタは `Option<extern "C" fn(…)>`(null と同じ表現になる)で表す。
- グルーは呼ぶたびに `CapsuleMap` を新しく作る。利用者の関数の型(`fn(CapsuleMap) -> JuizResult<Capsule>`、`&mut ContainerImpl<T>` を取る関数、`JuizResult<T>` を返す型付きの関数)は変えない。
- プラグインの中の `log` は、今までどおりプラグイン自身の logger(`env_logger::try_init()`)に出る。ホストの logger につなぐのはこの文書の範囲外(§13)。
- 入口の関数は複数のスレッドから同時に呼ばれてよい。今の Rust ABI と同じく、利用者の Process の関数は `Send + Sync` 相当でなければならない。コンテナプロセスは、ホストがコンテナのロックを持って呼ぶので同時には呼ばれない(§8.3)。

## 8. コンテナ

### 8.1 作成と破棄

```c
int64_t (*create)(juiz_bytes manifest_json, const juiz_frame_in* args, void** handle, const juiz_bytes_sink* error);
void (*destroy)(void* handle);
```

- `manifest_json` はホストが作るコンテナの `ContainerManifest`(名前などを埋めたもの)の JSON。グルーは自分の juiz_sdk の `ContainerManifest` に読む。読めなければ `JUIZ_RUST_FRAME_ERROR`。
- グルーは利用者の作成関数(`fn(CapsuleMap) -> JuizResult<Box<T>>`)を呼び、**プラグインの中で** `ContainerImpl<T>` を作り、`Box::into_raw` したものを `*handle` に入れて 0 を返す。
- `handle` は不透明なポインタで、ホストは中身を見ない。ホストは自分の側に入れ物(`RustCabiContainer { handle, destroy, container_tag, lib }`)を作り、今の C++ の `CppContainerStruct` と同じく `ContainerImpl<RustCabiContainer>` として登録する。ホスト側のコンテナの profile・Process の一覧はこの入れ物が持つ。
- ホストはコンテナを消すとき、`destroy(handle)` を 1 回だけ呼ぶ。`destroy` は `Box::from_raw` して落とす。入れ物は `Arc<Library>` を持ち、`destroy` を呼ぶまでプラグインを外さない。
- プラグインの中の `ContainerImpl<T>` の、ホストの状態を映す部分(`processes()`・`process()`・`parent_container`)は空になる。コンテナプロセスの中からそれらを使っていた利用者のコードは、C ABI では空を見る。juiz の examples・test_packages と juiz_examples_rust には無いことを Step 1 で確かめた。

### 8.2 コンテナの型タグ

```text
rust:<rustc -V の出力>:<juiz_sdk の版>:<std::any::type_name::<ContainerImpl<T>>()>:<size_of>:<align_of>
```

- 今の `ContainerTypeInfo::of::<ContainerImpl<T>>()`(型の名前・juiz_sdk の版・サイズ・アラインメント)に rustc を足したもの。juiz_sdk の `juiz_sdk::cabi::container_tag::<T>()` が作る。
- 不透明な文字列として完全一致だけで比べる(区切りの `:` で分けて読まない。型の名前に `::` が入るため)。
- Container の表の `info` は作るコンテナの型タグを、ContainerProcess の表の `info` は受け取るコンテナの型タグを返す。

### 8.3 コンテナプロセスの呼び出し

```c
int64_t (*container_call)(void* handle, const juiz_frame_in* args, const juiz_frame_out* out);
```

1. ホストはコンテナのロック(`ContainerPtr::lock_mut`)を取り、入れ物が `RustCabiContainer` であることを確かめる。
2. 入れ物の `container_tag` と、コンテナプロセスの表の `container_tag` を比べる。違えば呼ばずに `ContainerTypeMismatchError { expected: <プロセス側>, actual: <コンテナ側> }` にする。
3. 同じなら `container_call(handle, …)` を呼ぶ。グルーは `handle` を `&mut ContainerImpl<T>` に戻して利用者の関数を呼ぶ。戻り値は §5.3。
4. ロックは呼び出しが終わるまで持つ。同じコンテナのコンテナプロセスは同時に呼ばれない。

- コンテナとコンテナプロセスが **同じプラグイン** にあれば型タグは必ず一致する。
- **別のプラグイン**(examples の `example_container` と `example_container_increment` のように、別々のクレート)にあるときは、2 つが同じ rustc と同じ juiz_sdk の版でビルドされ、`ContainerImpl<T>` の名前と大きさが同じときだけ呼べる。同じパッケージの中なら、作った人が同じ rustc でビルドするので問題にならない。
- 型タグは依存クレートの版(`T` が持つ serde_json などの型の配置)までは見ない。2 つのクレートを同じワークスペース(同じ `Cargo.lock`)でビルドすることを勧める(サイトとサンプルに書く、Step 5)。
- コンテナが C ABI で、コンテナプロセスが Rust ABI(またはその逆)のときは、ホスト側の入れ物の型が合わないので `ContainerDowncastingError` になる。エラーの文で、両方を同じ juiz_sdk でビルドし直すよう案内する。
- コンテナスタック(§12)のコンテナプロセスが親コンテナの中身を `ContainerImpl<T>` として読む(examples の `example_container_stack_get`)ときは、親を C ABI で読むとホストからは中身が見えない。そのときは親とそのコンテナプロセスを conf の `abi: rust`(§11)で読む。

## 9. ABI の版

### 9.1 版を上げるとき

- 上げる: 入口の表の既存の項目の並び・型・意味を変える、§4 の構造体を変える、フレームの使い方(§5)の必須の規則を変える、エラーコードの意味を変える。
- 上げない: 表の後ろに項目を足す(`struct_size` で見分ける)、ヘッダに任意のキーを足す(知らないキーは無視する)、エラーコードを足す。

### 9.2 ホストの検査

- ホストは自分が読める ABI の版の集合を持つ。最初は `{1}`。
- 読み込むとき、`<symbol>_cabi` の表の `abi_version` が集合に無ければ `RustPluginAbiVersionError` にする。rustc・juiz_sdk の版は比べず、`info` の `build_info` をログ(debug)に出す。
- `juiz_rust_abi_version() -> u32`(`#[no_mangle] extern "C"`、juiz_sdk の `juiz_sdk::cabi`)は、そのプラグインが作る表の版を返す。ホストの読み込みには使わず(表の `abi_version` を見る)、道具(将来の `juiz plugin info` など)と試験のために置く。cdylib から依存クレートの `#[no_mangle]` の関数が外に見えることは Step 1 で確かめた(Linux の `nm -D` と、juiz_core の統合テスト `rust_cabi_entry_test` で libloading から引けること。macOS・Windows も同じテストが CI で確かめる)。

### 9.3 ファイルの中の印

- 表の `abi_marker` は `"juiz_rust_abi=<版>;"` の C 文字列で、表から参照されるのでリンカに消されない。
- `juiz pack` はプラグインを読み込まずに、ファイルのバイト列から `juiz_rust_abi=` を探して版を読む(今の `juiz_sdk=…;rustc=…` を探すのと同じやり方、app-package.md §6.3)。別ターゲットのバイナリでも読める。

## 10. Native(遅延評価 L2)を運ぶ(Step 3 #331)

Native(`RustNative<T>` など)はプラグインの中に置いたまま、ハンドルと関数の表をホストに渡す。C++ の L3(datatype.md §21)と同じく、表は値と一緒に運ぶ。これで Composite の中間値を同じプラグインへ渡すときに encode が起きない。Step 2 までは境界で encode していた(決めごと C)。

### 10.1 構造体

```c
typedef struct juiz_rust_native_vtable {
    uint32_t abi_version;     /* 1 */
    uint32_t struct_size;
    /* 共通形式にして out に書く。ヘッダは kind: response で、result がその値 */
    int64_t (*encode)(const void* handle, const juiz_frame_out* out);
    /* 解放する。1 回だけ呼ぶ */
    void (*destroy)(void* handle);
} juiz_rust_native_vtable;

typedef struct juiz_rust_native {
    void* handle;                              /* プラグインの中の Box<NativeCell> */
    const juiz_rust_native_vtable* vtable;     /* プラグインの中の static */
    juiz_bytes type_tag;                       /* {"name": …, "fingerprint": …, "abi": …} の JSON */
    uint32_t kind;                             /* 0 Value / 1 Image(NativeKind、encode せずに答える) */
    uint32_t owned;                            /* 引数のとき: 1 なら持ち主がプラグインに移る */
} juiz_rust_native;
```

- 表(vtable)は値と一緒に運び、登録所には入れない(C++ の L3 と同じ、lazy-evaluation.md §8)。juiz_sdk の `juiz_sdk::cabi` が static に 1 つ持ち、juiz_sdk はプラグインごとに静的にリンクされるので、表のアドレスはプラグインごとに違う。
- `handle` はプラグインの中の `Box<NativeCell>`。`NativeCell` は `Arc` なので、複製しても値は共有する。
- `type_tag` は `{"name": …, "fingerprint": …, "abi": …}` の JSON。`abi` はプラグインの `native_abi_tag`(§3.3)。型タグを持たない Native は `{"name": <表示用の型名>}` だけにする。ほかの `juiz_bytes` と同じく呼び出しの間だけ有効で、受け取った側は複製する。
- 表の `encode` は、`kind: response` で `result` がその値のヘッダと本体を `out` に書く(§5.3 と同じ書き方。`out->native` は null で渡す)。プラグインの側では結果を覚えない(ホストの `NativeCell` が覚えるので二重にしない)。すでに覚えている(プラグインの中で encode 済みの)ときはそれを使う。
- 表の関数も `catch_unwind` で囲む(§6.2)。`destroy` の panic はログに出して握りつぶす。

### 10.2 戻り値の Native

- 利用者の関数が `Capsule::Native`(`RustNative<T>`)を返し、ホストが `out->native` を持つ(null でない)なら、グルーはヘッダの `result` を `{"native": 0}` にして `out->begin` を呼び、`out->native(ctx, 0, native)` で渡す。`handle` は `Box::into_raw(Box::new(native_cell))`、`owned` は 1。カプセルのオプションはヘッダの `option` に入れる。
- `native` が 0 を返したら、持ち主はホストに移る。負の値を返したら持ち主はプラグインのままで、グルーが解放して `JUIZ_RUST_HOST_REFUSED` を返す。
- 番号は 0 から順に渡す(ホストは順でない番号を断る)。今のグルーが渡すのは戻り値の 1 つだけ。
- ホストは受け取ったものを `RustCabiNative`(juiz_core の `plugin/rust/cabi_host.rs`)に包み、`NativeValue` として `Capsule::Native` に入れる。`language()` は `rust`、`kind()` は受け取った `kind`、`type_name()` は型タグの `name`、`type_tag()` は型タグ(3 つ揃っているときだけ)、`encode()` は `vtable->encode` を呼んでフレームを読む。`Drop` で `destroy` を呼ぶ。作ったプラグインの `native_abi_tag` と `Arc<Library>` を持つ。
- ホストは受け取る前に表を確かめる(`abi_version` が読める版、`struct_size` が足りる、`encode` と `destroy` が null でない、`kind` が 0 か 1)。合わなければ断る(負の値を返す)。
- ホスト自身やブローカー・ほかの言語・Rust ABI で読んだプラグインが値を要るときは、今までどおり encode する(lazy-evaluation.md §5)。encode の結果はホストの `NativeCell` が覚えるので、表の `encode` は値ごとに高々 1 回。

### 10.3 引数の Native

ホストは引数の Native ごとに、渡す先のプラグインの `native_abi_tag` と比べる。比べるのは、値を作ったプラグインの `native_abi_tag`(`RustCabiNative` が持つもの)で、型タグの `abi` ではない(型タグを持たない Native もあるため。型タグを持つときは同じ値になる)。

- 値が `RustCabiNative` で、作ったプラグインの `native_abi_tag` が渡す先のものと同じ(= 同じプラグインが作った値): ハンドルのまま渡す。ヘッダの値は `{"native": <番号>}`、`args->natives[<番号>]` にその値を入れる。
  - ホストがほかに参照を持たない(`NativeCell::try_into_value` が成功する。Composite の中間値)なら `owned = 1` にし、ホストの入れ物は `destroy` を呼ばずに捨てる。プラグインのグルーは `Box::from_raw` して自分の `NativeCell` を取り戻す。`native_extract` が値を**移す**(clone も encode もしない)。
  - 参照を持つ(接続の memo を共有している)なら `owned = 0` にし、ホストが持ち続ける。グルーは `handle` の `NativeCell` を `clone`(`Arc` の複製)して使い、`native_extract` が `T` を `clone` する。呼び出しの後は `handle` に触らない。
- それ以外(別のプラグイン、ほかの言語、ホストが作った値、Rust ABI で読んだプラグインの値): encode して Value / Image として渡す。
- `owned = 1` の Native は、ホストが入口を呼んだ時点で持ち主がプラグインに移る。入口がどの値を返しても(エラー・panic・`JUIZ_RUST_NULL_ARGUMENT` でも)、プラグインが解放する。そのためグルーは、ほかの検査より先に Native を受け取る。入口を呼べなかった(フレームを作れなかった)ときは、ホストが `destroy` を呼ぶ。
- グルーは、受け取った Native の `vtable` が自分の表(同じプラグインの static)と同じことを確かめる。違えば `JUIZ_RUST_FRAME_ERROR`(`native <番号> was not made by this plugin`)にし、`owned = 1` のものはその表の `destroy` で解放する。ホストの照合が正しければ起きない。
- Process・コンテナの作成・コンテナプロセスの 3 つの入口で同じ。

これで今の L2 の性質(Composite で移しても encode しない、接続では clone 1 回、別のプラグインへは辞書)を C ABI でも保つ。`rust_native_test` は C ABI と Rust ABI(`abi: rust`)の両方で同じことを確かめ、表の `encode` を呼んだ回数(`juiz_core::rust_cabi_native_encode_count()`、試験用)で、Composite と接続で encode が起きないことも確かめる。

## 11. 移行(決めごと A)

- ホストは各ファクトリについて、まず `<symbol>_cabi` を探す。あれば C ABI で読み、`BuildInfo` の検査(`check_build_info`)はしない。
- 無ければ今の Rust ABI の `<symbol>` を、今の `BuildInfo` の検査付きで読む(古いプラグイン)。読み込めたら info で `loaded <path> through the Rust ABI. Rebuild it with juiz_sdk <ホストの版> or later to load it through the C ABI.` を一度出す。
- conf のプラグインの項目に `abi: rust` を書くと、`_cabi` の入口があっても今の Rust ABI で読む(`BuildInfo` の検査も今のまま)。`abi: c`(または書かない)は上の既定の振る舞い、それ以外の値は `InvalidSettingError`。移行期間の逃げ道で、次の 2 つに使う(Step 2 で追加)。
  - Step 2 までは、Native をハンドルのまま運ぶ L2 の性質(lazy-evaluation.md §9)が要るときにも使った。Step 3(§10)からは C ABI でも Native をハンドルのまま運ぶので要らない
  - コンテナスタックの親コンテナ(§8.3)
- `abi` は、そのプラグインの項目(`process_factories` / `container_factories` とその `processes` / `components` の各項目)ごとに書く。コンテナとそのコンテナプロセスは同じ `abi` にしなければならない(§8.3)
- juiz_macro は移行期間のあいだ両方の入口を生成する(§3.1)。新しい juiz_sdk でビルドしたプラグインは、古い juiz(同じ rustc)でも新しい juiz(違う rustc でも)でも読める。
- Rust ABI の読み込みと、マクロの Rust ABI の入口の生成をいつやめるかは後で決める(別 issue)。やめるときは ABI の版は上げない(C ABI 側は変わらないため)。
- EC・ブローカーのプラグインは移行の対象ではない(§12)。今の読み込みと、`BuildInfo` が無いときに警告だけで読む振る舞いも変えない。

## 12. 対象と対象外

| もの | 境界 | 理由 |
|---|---|---|
| Process(`#[juiz_process]`・`#[juiz_component_process]`) | C ABI | |
| Container(`#[juiz_container]`・`#[juiz_component_container]`) | C ABI | |
| ContainerProcess(`#[juiz_container_process]`・`#[juiz_component_container_process]`) | C ABI | |
| Component(`juiz_component_manifest!`) | C ABI | |
| EC のプラグイン(`ec_factories`) | Rust ABI のまま | juiz_core の trait(`ExecutionContextFunction` など)を直接渡すため |
| ブローカーのプラグイン(`broker_factories`、`broker_factory` / `broker_proxy_factory`) | Rust ABI のまま | `CoreBrokerPtr`・`BrokerFactory` の trait を直接渡すため |
| コンテナスタック(`container_stack_factory`、マクロを使わず juiz_core を直接使う) | Rust ABI のまま | 親コンテナの `ContainerPtr` を直接受け取るため |
| `plugin_manager`(どこでも使っていない) | 変えない | |

EC・ブローカーのプラグインは今の `BuildInfo` の検査を続け、.juizapp でも rustc を検査する(app-package.md §12.1)。

## 13. 範囲外(後の Step か、未定)

- プラグインの中の `log` をホストの logger に流す(C の関数でログを渡す形)。今はプラグインごとの logger のまま
- 実行中にプラグインを外す(lazy-evaluation.md §10 と同じく、外さない前提)
- C ABI のプラグインを Rust 以外(C++ など)が直接書けるようにすること。この表の形は Rust のグルー専用で、C++ は今の libjuiz_sdk の C ABI を使う
- EC・ブローカーの C ABI 化
- Rust ABI の読み込みをやめる版(§11)

## 14. Step との対応

| Step | issue | この文書の節 |
|---|---|---|
| 0 仕様書 | #328 | 全体、app-package.md §4.1・§4.3・§6.3・§9・§12・§12.1 |
| 1 SDK のグルーとマクロ | #329 | §3〜§5・§6.1・§6.2・§7・§8.1・§8.2・§9.2・§9.3。フレームを juiz_sdk に移す。グルーの単体テスト(フレームの往復、panic がコードになる、manifest の panic) |
| 2 ホストの読み込み | #330 | §3.2・§6.3・§8・§9.2・§11。`_cabi` の検出、Rust ABI へのフォールバック、コンテナの型タグの照合、エラー。`-V` だけ偽の版を返す rustc でビルドしたプラグインが読めて呼べることを確かめるテスト。実装は juiz_core の `plugin/rust/cabi_host.rs`。偽の rustc は `juiz_sdk/test_packages/fake_rustc`、`scripts/test.sh` が `target/fake-rustc` にプラグインを作り、統合テスト `rust_cabi_load_test` が読む。conf の `abi: rust`(§11)を追加 |
| 3 Native | #331 | §10。グルーの Native の表・`{"native": <番号>}`、ホストの `RustCabiNative` と引数の受け渡し。`rust_native_test` を C ABI と Rust ABI の両方で通す。`rust_cabi_entry_test` と `typed_process_test` で入口を直接呼んで、ハンドルの受け渡しと所有権を確かめる。試験用のホスト `juiz_sdk::cabi::host` に Native の受け渡しを足した |
| 4 .juizapp | #332 | app-package.md §4.1・§4.3・§6.3・§9・§12.1 |
| 5 ベンチ・サンプル・サイト・リリース | #333 | 呼び出しのオーバーヘッドと 720p・4K 画像の数字(large-data.md §1.1 の「同じプロセス内の call」と比べる)、examples_rust、サイトの文 |
| 6 大きな画像を速くする | #339 | §5.3・§5.5・§16。領域の 0 埋めの省略と、プラグイン側・ホスト側の画像の領域の使い回し。ベンチは測るものごとに子のプロセスで測る形にした |

## 15. 計画から変えたこと・細かく決めたこと

- **ABI の版は入口の表に持たせた。** 計画では各入口が `juiz_rust_abi_version()` を持つとしていたが、Component では 1 つのクレートに複数のマクロが並び、マクロごとに同じ名前の関数を生成するとぶつかる。そこで表の先頭の `abi_version` をホストが見ることにし、`juiz_rust_abi_version()` は juiz_sdk が 1 つだけ持つ補助にした(§9.2)。
- **入口はファクトリごとに 1 つの表にした。** manifest・呼び出し・作成・破棄を別々のシンボルにせず、`<symbol>_cabi` が返す表にまとめた。シンボルを探す回数が減り、後から項目を足せる(`struct_size`)。
- **戻り値の書き込み口は `begin` と `body` の 2 段にした。** ホストがヘッダを読んで最終的な領域を確保し、プラグインはそこに直接書く。これで帰りの画像のコピーが 1 回で済む。
- **画像の `stride` の余白は認めない。** 境界の両側が Rust の `DynamicImage` なので余白は生まれず、認めると余白を取るコピーが増えるため。
- **コンテナの handle はプラグインの中の `ContainerImpl<T>`。** 利用者の関数が `&mut ContainerImpl<T>` を取る形を変えないため。そのぶん、プラグインの中の `ContainerImpl` はホストの Process の一覧を持たない(§8.1)。
- **Rust ABI の入口も当面生成し続ける。** 古い juiz でも新しいプラグインが(同じ rustc なら)読めるようにするため(§11)。
- **Native の照合はプラグインの `native_abi_tag` で行う。** 型タグの `abi` は型タグを持つ Native にしか無いため、作ったプラグインの `native_abi_tag`(info、§3.3)をホストが値と一緒に覚えて比べる(§10.3)。
- **`owned = 1` の Native は入口を呼んだ時点で持ち主が移る。** 戻り値のコードで持ち主が変わると、エラーの道筋ごとに解放の責任が分かれて漏れやすいため(§10.3)。戻り値の Native も同じく、`native` が 0 を返した時点でホストに移る(§10.2)。

## 16. 呼び出しの時間(Step 5 #333)

`juiz_core/tests/integration/rust_abi_bench.rs`(`#[ignore]`、release で実行)で、同じプラグインを conf の `abi` だけ変えて CoreBroker の `process_call` の 1 回あたりの時間を測った。small は `increment_process` に整数、720p・4K は `image_echo_proc`(画像をそのまま返す)に RGB の画像を渡す。クラウドの Xeon 4 コア、5 回中の最短。

| | C ABI | Rust ABI | 差 | 画像の複製 1 回 |
|---|---|---|---|---|
| small [µs] | 4.0 | 1.2 | 2.8 | - |
| 720p [ms] | 6.0〜7.2 | 0.23 | 約 6.5 | 0.22 |
| 4K [ms] | 33 | 4.0 | 29 | 4.0 |
| 720p [ms](glibc がメモリを返さない設定) | 1.1 | 0.22 | 0.87 | 0.23 |
| 4K [ms](同上) | 19 | 4.1 | 15 | 4.1 |

- 小さい値の呼び出しは約 3µs 増える(ヘッダの JSON の読み書きと、表を通した呼び出し)。
- 画像は、行き(プラグインが自分の `DynamicImage` に読む)と帰り(ホストが確保した領域にプラグインが書く)で 1 回ずつ複製し、ホストの領域を 0 で埋める。Rust ABI は Capsule をそのまま渡すので、ベンチの中の複製 1 回だけになる。
- 既定の glibc では、時間の半分ほどがカーネル(大きな領域を確保・解放するたびのページフォールト)だった。`MALLOC_TRIM_THRESHOLD_` などで glibc がメモリを OS に返さないようにすると「同上」の行になる。領域の使い回しと 0 埋めの省略は Step 6(#339、下の 16.1)で行った。
- 同じ rustc でビルドしていて画像を多く運ぶプラグインは、conf の `abi: rust`(§11)で今までの速さのまま読める。
- このベンチを作るときに、C ABI と関係なく画像の引数を受け取るたびに Capsule 全体(画素すべて)を Debug の文字列にしていた無駄(`CapsulePtr::extract_image`、720p で約 75ms)と、プラグインが引数の画像を読むときの 0 埋めを直した。

### 16.1 領域の使い回しの後(Step 6 #339)

§5.5 の使い回しと §5.3 の 0 埋めの省略の前後で、同じベンチを同じ機械(クラウドの Xeon 4 コア、上の表とは別の日・別の機械)で測った。glibc の malloc は、それまでに確保・解放した大きさで振る舞い(mmap にするか、OS に返すか)を変え、同じプロセスで続けて測ると前に測ったものの影響を受けたので、ベンチは C ABI・Rust ABI・複製をそれぞれ子のプロセスで測る形にした。既定の glibc の設定のまま、2 回測った値。

| | 前: C ABI | 後: C ABI | Rust ABI | 画像の複製 1 回 |
|---|---|---|---|---|
| small [µs] | 3.2 | 3.3〜3.4 | 0.9 | - |
| 720p [ms] | 0.79〜3.7 | 0.66〜0.67 | 0.21 | 0.20〜0.21 |
| 4K [ms] | 20〜22 | 6.0〜6.1 | 1.95 | 1.95 |

- 後の C ABI は、ほぼ複製 3 回ぶん(ベンチの中の複製、行き、帰り)になった。ページフォールトは無くなり、残りの差(720p で約 0.45ms、4K で約 4ms)は境界の両側に `DynamicImage` を持つための行きと帰りの複製 2 回で、この形(§2)では減らせない。
- 前の 720p が 0.79〜3.7ms と揺れるのは、glibc がその時々に領域を OS に返すかどうかで変わるため。
- 画像を多く運び、複製 2 回も惜しいプラグインは、今までどおり conf の `abi: rust`(§11)で読める(同じ rustc でビルドしていること)。

