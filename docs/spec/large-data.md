# 大きなデータの受け渡し(生バイトのフレームと共有メモリ)仕様

- 状態: 確定(大きなデータ Step 2、issue #232、2026-10-28)
- 親 issue: #229。計画と経緯はプロジェクトの計画書 ipc-broker-review.md(2026-10-28 確定)にある。基準の数字は #230 のベンチ(`juiz_core/tests/integration/image_transfer_bench.rs`)
- 関連: IPC ブローカーの仕様 [ipc-broker.md](ipc-broker.md)、データ型の仕様 [datatype.md](datatype.md) §3.3・§4、遅延評価の仕様 [lazy-evaluation.md](lazy-evaluation.md)
- この文書は #233(IPC の作り直しと A)、#234(qmp に A)、#235(共有メモリ B)、#236(各言語)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と用語

同じマシンの別プロセス(と、qmp で別マシン)へ、カメラ画像などの大きなデータを速く渡す。標準は 1280x720(720p)の RGB 画像、最大は 3840x2160(4K)を想定する。

| 用語 | 意味 |
|---|---|
| フレーム | 1 回の要求または応答を表すバイト列。JSON のヘッダと、0 個以上の本体からなる(§3) |
| ヘッダ | フレームの先頭にある JSON。関数名や識別子、小さな値を入れる |
| 本体 | ヘッダの外に生バイトのまま置く大きな値。画像の画素や数値の配列 |
| A | 本体を生バイトのままフレームに入れて送る方式(§3) |
| B | 本体を共有メモリのリングに書き、フレームには場所だけを入れて送る方式(§6) |
| C | 受け手が共有メモリを指したまま値を使い、コピーもしない方式。この文書では扱わない(§9) |
| リング | B で送り手が持つ共有メモリの領域。決まった数の枠を順に上書きして使う |
| 枠 | リングの中の 1 フレーム分の場所 |

### 1.1 基準の数字(#230、クラウドの Xeon 4 コア、release、1 回あたり ms)

| | 生 / PNG [MB] | HTTP の call の戻り値 | うち PNG 圧縮 | うち PNG 展開 | 生バイトを Unix ソケットで | 同じプロセス内の call |
|---|---|---|---|---|---|---|
| 720p | 2.76 / 1.84 | 24.1 | 10.9 | 8.0 | 0.63 | 0.23 |
| 1080p | 6.22 / 4.13 | 52.5 | 25.7 | 18.6 | 1.6 | 0.59 |
| 4K | 24.88 / 16.51 | 195.3 | 96.2 | 79.0 | 15.7 | 4.3 |

HTTP の時間の約 8 割が PNG の圧縮と展開なので、A で PNG をやめれば 720p・1080p は数 ms に収まる。4K は生バイトでも 16ms かかる(25MB を毎回新しく確保するページフォールトを含む)ので、B で受け持つ。

## 2. 基本方針

1. **A で 720p・1080p をまかなう。** IPC ブローカー(#233)と qmp ブローカー(#234)は同じフレーム形式(§3)を使う。画像は PNG にせず、画素をそのまま送る。
2. **B で 4K を受け持つ。** 同じマシンの IPC だけで使う。まず Topic の push に入れる(§6)。Topic ごとに conf で枠を設定する。受け手が遅いときは古いフレームを捨てる。
3. **HTTP は PNG のまま。** ブラウザや curl から使えることを優先する。ただし今の不具合(§7)は直す。
4. **カプセルの種類は増やさない。** 受け手は今と同じ `Capsule::Value` / `Capsule::Image` を作る。受け手側のコピーは 1 回残る(なくすのは C)。
5. **値の意味は変えない。** フレームで送っても、HTTP や同じプロセス内で渡したときと同じ値が受け手に届かなければならない(数値の整数と小数の区別、画像の画素形式を含む)。

## 3. フレーム形式(A)

### 3.1 バイト列

数はすべてリトルエンディアン。

```text
+--------+-------------+---------------------+-----------+---------+-----------+---------+---
| "JZF1" | header_len  | header (UTF-8 JSON) | body_len0 | body0   | body_len1 | body1   | ...
| 4 byte | u32         | header_len byte     | u64       | ...     | u64       | ...     |
+--------+-------------+---------------------+-----------+---------+-----------+---------+---
```

- 先頭の 4 バイト `JZF1` は形式とその版を表す。違う値が来たら、受け手はエラーにして接続を閉じなければならない(続きの区切りがわからないので、エラー応答は返さない)。
- 本体の数は、ヘッダの `bodies` の要素の数(§3.2)。本体はその順に並ぶ。
- 本体の位置合わせ(パディング)はしない。
- 本体が共有メモリにあるとき(B、§6.4)、その本体の `body_len` は 0 で、バイトは続かない。

### 3.2 ヘッダ

要求:

```json
{
  "id": 17,
  "kind": "request",
  "param": {
    "method_name": "update",
    "class_name": "process",
    "function_name": "call",
    "identifier": "ipc://cam0/process/capture0::capture"
  },
  "map": {
    "image": {"body": 0},
    "gain": {"value": 1.5}
  },
  "bodies": [
    {"kind": "image", "width": 1280, "height": 720, "format": "rgb8", "stride": 3840}
  ]
}
```

応答:

```json
{
  "id": 17,
  "kind": "response",
  "result": {"body": 0},
  "option": {"function_name": "call", "class_name": "process"},
  "bodies": [
    {"kind": "image", "width": 1280, "height": 720, "format": "rgb8", "stride": 3840}
  ]
}
```

エラー応答:

```json
{
  "id": 17,
  "kind": "error",
  "error": {"class_name": "process", "function_name": "call", "message": "..."}
}
```

| キー | 意味 |
|---|---|
| `id` | 要求ごとに送り手が付ける通し番号(u64)。応答は同じ `id` を返さなければならない。受け手は合わなければエラーにして接続を閉じる |
| `kind` | `request` / `response` / `error` |
| `param` | 今の `CapsuleMap` の `__param__` と同じもの。`method_name`(`create` / `read` / `update` / `delete`、小文字)・`class_name`・`function_name` を必ず含む。値はすべて文字列 |
| `map` | 今の `CapsuleMap` の `__map__` にあたる。キーは引数名、値はカプセルの表し方(§3.3) |
| `result` | 応答のカプセル(§3.3) |
| `option` | 応答のカプセルのオプション(`CapsulePtr::set_option` で付けたもの)。無ければ省いてよい |
| `bodies` | 本体の説明の配列(§3.4)。本体が無ければ省いてよい |
| `error` | エラーの内容。受け手の BrokerProxy は `JuizError::BrokerProxyRemoteError { class_name, function_name, message }` にする(#231 と同じ) |

`param` と `map` を今の形に合わせるのは、受け手の `CRUDBroker` の処理(`on_value_request` と同じ振り分け)をそのまま使うため。受け手は `Value` を経由せずに `CapsuleMap` を組み立て、画像のカプセルを直接入れなければならない。

### 3.3 カプセルの表し方

| 形 | 意味 |
|---|---|
| `{"value": <JSON>}` | `Capsule::Value`。値は datatype.md §4 の辞書の表し方(`bytes` は base64 の文字列) |
| `{"empty": true}` | `Capsule::Empty` |
| `{"body": <番号>}` | `bodies` の何番目の本体か。画像か数値の配列(§3.4) |

- `Capsule::Native` は送る前に encode し(lazy-evaluation.md §2)、結果の Value / Image として送る。
- 本体を参照できるのは `map` の値と `result` だけ。Value の中の一部(辞書のフィールドや配列の要素)を本体に出すことはしない(§9)。

### 3.4 本体の種類

#### 画像 `image`

`Capsule::Image`(`DynamicImage`)を送る。PNG などに圧縮しない。

```json
{"kind": "image", "width": 1280, "height": 720, "format": "rgb8", "stride": 3840}
```

| `format` | `DynamicImage` | 1 画素のバイト数 | 1 チャンネルの型 |
|---|---|---|---|
| `l8` | `ImageLuma8` | 1 | u8 |
| `la8` | `ImageLumaA8` | 2 | u8 |
| `rgb8` | `ImageRgb8` | 3 | u8 |
| `rgba8` | `ImageRgba8` | 4 | u8 |
| `l16` | `ImageLuma16` | 2 | u16(リトルエンディアン) |
| `la16` | `ImageLumaA16` | 4 | u16 |
| `rgb16` | `ImageRgb16` | 6 | u16 |
| `rgba16` | `ImageRgba16` | 8 | u16 |
| `rgb32f` | `ImageRgb32F` | 12 | f32(リトルエンディアン) |
| `rgba32f` | `ImageRgba32F` | 16 | f32 |

- `stride` は 1 行のバイト数。`width × 1 画素のバイト数` 以上でなければならない。本体の長さは `stride × height` でなければならない。合わなければエラーにする。
- Rust の送り手は `DynamicImage::as_bytes()` をそのまま書き、`stride` は `width × 1 画素のバイト数` にする(余白なし)。余白のある `stride` は、行の終わりに余白を持つ画像(C++ の `cv::Mat` など)の送り手のためにある。受け手は余白を取り除いて `DynamicImage` を作る。
- 上の表にない形式(BGR など)は送れない。足すときはこの表を直す(C++ の画像型を足す #236 で `bgr8` などを検討する)。

#### 数値の配列 `array`

Value がまるごと数の配列で、要素が多いとき、JSON の数値の列ではなく生バイトで送る。点群などを想定する。

```json
{"kind": "array", "dtype": "f64le", "count": 300000}
```

| `dtype` | 要素の条件 | 1 要素のバイト数 |
|---|---|---|
| `i64le` | すべて i64 で表せる整数(serde_json の `as_i64()` が値を返す) | 8 |
| `f64le` | すべて小数(serde_json の `is_f64()` が真) | 8 |

- 送り手は、`map` の値か `result` が配列で、要素の数が 1024 以上で、上のどちらかの条件を満たすとき、本体に出してよい。整数と小数が混ざっている配列は本体に出さない(整数を小数にすると値の意味が変わるため、§2 の 5)。
- 受け手は `Value::Array` を作り、`i64le` の要素は整数、`f64le` の要素は小数にしなければならない。受け手は必ず両方を受け付けなければならない。
- 本体の長さは `count × 8` でなければならない。

#### `bytes` を本体にしない理由

datatype.md の `bytes` は Value の中では base64 の文字列なので、送り手はほかの文字列と見分けられない。`bytes` を生バイトで送るのは、バイト列のカプセルができたとき(C か、その後)に検討する(§9)。

### 3.5 上限と不正なフレーム

| 項目 | 既定値 | 超えたとき |
|---|---|---|
| `header_len` | 64 MiB | エラー応答を返して接続を閉じる(残りを読み飛ばさない) |
| 1 フレームの合計(ヘッダ + 本体) | 1 GiB | 本体の長さはわかっているので読み飛ばし、エラー応答を返す。接続は保つ |

- 合計の上限は、ブローカーの manifest の `max_frame_bytes` で変えてよい。4K の `rgba32f`(約 133 MB)も既定で通る。
- ヘッダの JSON が読めない、必須のキーが無い、本体の数や長さが説明と合わないときは、エラー応答を返す。区切りが保てていれば接続は保つ。

### 3.6 書き方と読み方

- 送り手は本体を別のバッファに複製せず、ヘッダと本体をまとめて書く(`write_vectored` など)。画像は `as_bytes()` の参照をそのまま書く。
- 受け手は本体を、最終的な `DynamicImage` / `Vec` の領域へ直接読み込む(`read_exact`)。中間のバッファに読んでから複製しない。
- これで送り手と受け手のコピーは、カーネルとの間の 1 回ずつになる。

## 4. 通り道ごとの扱い

| 通り道 | 形式 | 担当 |
|---|---|---|
| IPC | §3 のフレームで要求と応答を送る。接続は使い回す(ipc-broker.md §5) | #233 |
| qmp | 要求ごとの双方向ストリームに、要求のフレームを 1 つ書いて閉じ、応答のフレームを 1 つ読む。今の MessagePack と `__qmp_error__` はやめる | #234 |
| HTTP | 今のまま(画像は PNG、multipart)。§7 の不具合は直す | #233 |
| 同じプロセス内 | 今のまま(カプセルを直接渡す) | — |

- qmp の新旧は互換でない。古い qmp と新しい qmp をつなぐと、先頭の 4 バイトが合わずにエラーになる。リリースノートに書く(#237)。
- ヘッダは qmp でも JSON にする。小さなヘッダでは JSON と MessagePack の差は無視でき、他の言語のクライアントを書きやすい。

## 5. Topic で Value 以外を通す

- Topic は `Capsule::Value` だけでなく `Capsule::Image` と `Capsule::Native` も通さなければならない。`Capsule::Empty` はエラーにする。
- Topic は受け取った `CapsulePtr` をそのまま(中身を複製せずに)転送先のブローカーと購読者に渡す。今の `capsule_ptr_to_capsule` のような値の複製はしない。
- ブローカー越しの push は、各ブローカーの形式で送る(IPC・qmp はフレーム、HTTP は PNG の multipart)。

## 6. 共有メモリ(B)

### 6.1 使う条件と conf

B は次のすべてを満たす push で使う。

1. Topic の push である(call の引数と戻り値には使わない。§9)。
2. 転送先のブローカーが IPC である(同じマシン)。
3. その Topic の conf に `shared_memory` がある。
4. 本体の長さが `min_bytes` 以上である。

満たさない push は A で送る。

conf にはトップレベルの `topics:` を足し、Topic ごとに書く。

```yaml
topics:
  - name: camera/image          # Topic の名前。ネームスペースの解決は Topic の名前と同じ (#149)
    shared_memory:
      slots: 4                  # 枠の数。既定 4、2 以上
      slot_bytes: 33554432      # 1 枠のバイト数。既定は auto (§6.7)
      min_bytes: 1048576        # これより小さい本体は A で送る。既定 1 MiB
```

- 設定は送り手(その Topic に publish する Process がある juiz)の conf に書く。受け手の conf は見ない。
- `topics:` に書いた名前の Topic がまだ無くても、Topic ができたときに設定を当てる。
- `shared_memory: {}` と書けば既定値で使う。

### 6.2 領域の配置

リングは送り手の juiz が Topic ごとに 1 つ持つ。購読者が複数いても、書き込みは 1 回で済む。

```text
offset 0                    : 領域ヘッダ (4096 byte)
offset 4096                 : 枠ヘッダ × slots (各 64 byte)
offset data_offset          : 枠 0 のデータ (slot_bytes)
offset data_offset + slot_bytes : 枠 1 のデータ
...
```

| 領域ヘッダ | 型 | 意味 |
|---|---|---|
| magic | `"JZSM"` | 形式 |
| version | u32 | 1 |
| slots | u32 | 枠の数 |
| slot_bytes | u64 | 1 枠のバイト数。4096 の倍数 |
| data_offset | u64 | 枠 0 のデータの位置。4096 の倍数 |
| region_id | 16 byte | 領域の UUID |

| 枠ヘッダ | 型 | 意味 |
|---|---|---|
| seq | `AtomicU64` | 書き込みの状態(§6.3) |
| len | u64 | 書いたバイト数 |

数はすべてリトルエンディアン(この領域は同じマシンの中だけで使う)。

### 6.3 書き込み

送り手は push ごとに通し番号 `n`(1 から)を進め、枠 `n % slots` に書く。枠ヘッダの `seq` で、書き込み中と書き終わりを区別する(シーケンスロック)。

1. `seq` に `2n - 1`(奇数 = 書き込み中)を Release で書く。
2. 本体をデータに書き、`len` を書く。
3. `seq` に `2n`(偶数 = 書き終わり)を Release で書く。

送り手は受け手を待たない。受け手がまだ読んでいる枠でも上書きする。

### 6.4 通知と読み出し

- 書き終わったら、送り手は購読者ごとの IPC 接続で、普通の Topic の push のフレームを送る。本体の説明に `shm` を付け、`body_len` は 0 にする(§3.1)。

```json
{"kind": "image", "width": 3840, "height": 2160, "format": "rgb8", "stride": 11520,
 "shm": {"region": "6f1c...", "slot": 1, "seq": 57, "len": 24883200}}
```

- 受け手は次の手順で本体を取り出す。
  1. 枠ヘッダの `seq` を Acquire で読み、`2n`(`n` は通知の `seq`)でなければ、上書きされたか書き込み中である(§6.5)。
  2. `len` バイトを自分の `DynamicImage` / `Vec` へコピーする。
  3. もう一度 `seq` を Acquire で読み、`2n` でなければ、コピーの最中に上書きされた。コピーした値は捨てる。
- 受け手は領域を読み取り専用で対応付けなければならない。受け手が送り手の領域を壊すことはできない。
- 受け手は `len`・`slot`・`stride × height` が領域の中に収まるかを確かめ、収まらなければエラーにする。

### 6.5 受け手が遅いとき

受け手は古いフレームを捨て、新しいフレームを優先する。

- 受け手の juiz は、IPC で shm 付きの Topic の push を受けたら、本体を読まずにすぐ応答を返す。通知(領域・枠・`seq` と説明)は Topic ごとの「最新の 1 件だけを持つ受け箱」に入れる。受け箱に前の通知が残っていたら、それを捨てて数える(`dropped`)。
- Topic ごとの配達のスレッドが受け箱から通知を取り出し、§6.4 の手順で本体を読んでから、今と同じく購読者へ配る。
- 読む前か読んでいる最中に上書きされていたら(§6.4 の 1 か 3)、そのフレームは捨てて数え(`overwritten`)、受け箱の次の通知を待つ。エラーにはしない。
- このため、B を使う Topic の push は、受け手の処理の終わりを待たない(A の push は今と同じく待つ)。

### 6.6 領域の作り方・渡し方・後始末

どの OS でも、送り手と受け手のプロセスが異常終了しても、ファイルや名前が残らないようにする。

| OS | 作り方 | 受け手への渡し方 | 後始末 |
|---|---|---|---|
| Linux | `memfd_create`(`MFD_CLOEXEC`・`MFD_ALLOW_SEALING`)、`ftruncate`、大きさを `F_SEAL_GROW`・`F_SEAL_SHRINK` で封じる | Unix ソケットの `SCM_RIGHTS` でファイル記述子を渡す | すべての記述子と対応付けが閉じれば OS が消す |
| macOS | `shm_open(O_CREAT \| O_EXCL, 0600)` を短い乱数の名前(31 バイト以内、例 `/jz.<pid>.<乱数>`)で作り、`ftruncate` の後すぐ `shm_unlink` | `SCM_RIGHTS` で記述子を渡す | 同上 |
| Windows | `CreateFileMappingW`(ページングファイル)、名前は `Local\juiz-shm-<region_id>`、作ったユーザーだけに許すセキュリティ記述子を付ける | 受け手が名前で `OpenFileMappingW` する | すべてのハンドルが閉じれば OS が消す |

- 領域を初めて使う接続では、送り手は最初の通知の前に、要求 `create` / `shm` / `attach`(`param` に `region`・`slots`・`slot_bytes`・`data_offset`)を送る。Unix ではこのフレームに記述子を添える。受け手は対応付けて、接続が閉じるまで持つ。
- 受け手は、知らない `region` を指す通知はエラーにする(送り手の不具合)。
- 送り手は、Topic が消えたときとブローカーを止めたときに領域を閉じる。受け手は接続が閉じたときに対応付けを外す。
- `SCM_RIGHTS` を送るために、Unix の IPC は `std::os::unix::net::UnixStream` の上に作るのがよい(ipc-broker.md §2.4)。名前で開く汎用のクレート(`shared_memory` など)は名前が残るので使わない。

### 6.7 枠が足りないとき

- `slot_bytes` が `auto` のとき、最初に B で送る本体の長さを 1 MiB の倍数に切り上げた値にする。
- 本体が `slot_bytes` より大きい push は A で送り、Topic ごとに最初の 1 回だけ警告をログに出して数える(`fallback`)。リングを作り直すことはしない(解像度を変えるなら conf に `slot_bytes` を書く)。

### 6.8 数え上げ

Topic の profile に、送り手と受け手のそれぞれで次の数を出さなければならない。管理画面と `juiz topic` で見られるようにする。

```json
"shared_memory": {"slots": 4, "slot_bytes": 33554432, "sent": 1200, "fallback": 0, "dropped": 3, "overwritten": 0}
```

## 7. 既知の不具合と、直した後の振る舞い(#233 で直した)

#230 のベンチで見つかった 3 件。どれも #233 で直した。

| # | 今 | 直した後 |
|---|---|---|
| 1 | HTTP で引数に画像を送ると、PNG が 2MB を超える(1080p 以上)と届かない。axum の本文の既定の上限 2MB に当たり、`multipart_to_capsule_map` の `unwrap()` で panic して接続が切れる | HTTP ブローカーの本文の上限を manifest の `max_body_bytes` で決める(既定 256 MiB、4K の PNG が通る)。multipart が読めないときや上限を超えたときは panic せず、400 / 413 とエラーの JSON を返す |
| 2 | `HttpBrokerProxy` で画像を引数に渡すと、`From<CapsuleMap> for Value` の `extract_value().unwrap()` で panic する | 引数に Value 以外があれば、サーバーが受け付けている multipart(画像は `image/png`)で送る。`From<CapsuleMap> for Value` は Value 以外で panic せず、変換できない値はエラーにする(`TryFrom` に変える) |
| 3 | Topic は Value 以外を通さず、同じプロセス内でも `CapsulePtr is not available for Topic` のエラーになる | §5 のとおり、Image と Native も通す。HTTP・IPC・qmp の push でも画像を送れる |

1・2 で HTTP は画像を送れるようになるが、PNG のままなので速くはならない(§4)。

#233 の実装で決めたこと:

- 1: `option.http_broker.max_body_bytes` を HTTP ブローカーに渡す。multipart の部分の `Content-Type: application/json` は、JSON として読めればその値に、読めなければ文字列のままにする(以前は常に文字列にしていたので、pyjuiz が画像と一緒に送った数値が文字列で届いていた)。
- 2: `HttpBrokerProxy` は、引数がすべて Value なら今までどおり PATCH の JSON、画像があれば PUT の multipart で送る。`From<CapsuleMap> for Value` は `TryFrom` に変えた。
- 3: Topic の Process は、受け取った `CapsulePtr` をそのまま返す(`ProcessImpl` に、カプセルを複製せずに返す関数を持てるようにした)。転送先のブローカーにも同じ `CapsulePtr` を渡す。

## 8. Step との対応

| Step | issue | この文書の範囲 |
|---|---|---|
| 3 | #233 | §3 のフレーム(IPC)、§4 の IPC と HTTP、§5、§7 |
| 4 | #234 | §4 の qmp |
| 5 | #235 | §6 |
| 6 | #236 | 他言語(pyjuiz の IPC クライアント、C++ / Haskell の画像型と `bgr8` など §3.4) |
| 8 | #237 | リリース(qmp の非互換をリリースノートに書く) |

目標の目安: 別プロセスへの call の戻り値で、720p は 2ms 以下、1080p は 4ms 以下(A)。#233 の IPC で 720p 1.2ms、1080p 3.0ms、4K 12.7ms(§1.1 と同じクラウドの Xeon 4 コア、release。HTTP の 12〜16 倍速い)。4K の Topic の push は、生バイトの Unix ソケット(16ms)より速いこと(B)。どれも #230 のベンチに IPC と B を足して確かめる。

## 9. 対象外

- **C(受け手でもコピーしない)。** 共有メモリを指したままのカプセル(例 `Capsule::SharedBuffer`)、Rust の借りた画像、Python の `numpy.frombuffer`、C++ の外部データの `cv::Mat`。遅延評価(`Native`)や型(`juiz.Image`)との関係を決める必要があり影響が広いので、A・B の効果を測ってから別に判断する。
- **B を call の引数と戻り値に使うこと。** 戻り値の寿命(いつ枠を再利用してよいか)を決める必要がある。Topic で効果を確かめてから検討する。
- **Value の一部を本体に出すこと。** 辞書のフィールドにある画像や配列。datatype.md §3.3 で `image` は引数・戻り値の型そのものとしてだけ使えるので、今は要らない。
- **`bytes` を生バイトで送ること**(§3.4)。
- **HTTP でフレームを使うこと。** 必要になったら、`Content-Type: application/x-juiz-frame` で受け付ける形で足す。
- **別マシンへの共有メモリ。** qmp は A だけ。
