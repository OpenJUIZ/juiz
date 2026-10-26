# 遅延評価(Capsule::Native)仕様

- 状態: 部分確定(L0、issue #174、2026-10-26)。§1〜§8・§10〜§12 は確定。§9(型識別子と照合)は #163 Step 0(指紋と互換性の決まり)の後に確定する
- 親 issue: #3。関連: #164(Composition)、#163(データ型)。計画と経緯はプロジェクトの計画書 composition-plan.md の §3.6・§7
- この文書は L1(#175、Python)の実装の基準になる。L2(#176、Rust)・L3(#177、C++)は §9 の確定後にこの文書を足して基準にする。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と用語

Process の戻り値を、**必要になるまで juiz の共通形式(`Value` / `Image`)に変換しない**ことを遅延評価と呼ぶ。同じ juiz プロセスの中で、同じ言語の Process 同士が値を受け渡すときは、変換(シリアライズ・デシリアライズ)を一度もしない。

| 用語 | 意味 |
|---|---|
| ネイティブ値 | 言語の中の値そのもの。Python のオブジェクト(`dict`、numpy の配列、PIL の画像など)、Rust の構造体、C++ の構造体 |
| Native | ネイティブ値を包んだ `Capsule` の中身の種類(`CapsuleValue::Native`)。今の `Empty` / `Value` / `Image` に足す |
| encode | ネイティブ値から共通形式(`Value` または `Image`)を作ること |
| decode | 共通形式から言語のネイティブ値を作ること(今の各プラグインの引数の変換) |
| codec | ある言語(または型)の encode / decode の組 |
| 言語タグ | ネイティブ値を作った言語の名前(`python`、`rust`、`cpp`)。受け取り側が「そのまま使えるか」を最初に判定する手がかり |
| 型識別子 | Rust / C++ のネイティブ値が同じメモリ上の並びかどうかを判定する値(§9)。Python では使わない |

今の受け渡し(接続でも Composite でも同じ):

```text
Python f の戻り値 (dict / ndarray) --encode--> Capsule::Value --decode--> Python g の引数
```

遅延評価の後:

```text
Python f の戻り値 --> Capsule::Native(python, PyObject) --そのまま--> Python g の引数
                                     \--encode(必要なときだけ、1 回)--> Rust h / ブローカー / 管理画面
```

## 2. 基本方針

1. **システムの形は変えない。** `ProcessImpl`、inlet / outlet、memo、接続、Composite の評価、ブローカー、ID、通信形式は今のまま。`Capsule` の中身の種類が 1 つ増えるだけ。
2. **encode は読む側が必要としたときに 1 回だけ起き、結果はキャッシュする。** 同じ値を何度読んでも encode は 1 回。
3. **共通形式を読む既存の API は、Native でも今と同じ結果を返す。** `is_value()`、`as_value()`、`lock_as_value()`、`extract_value()`、`is_image()`、C ABI のアクセサなどは、Native なら encode して今と同じ値を返す。既存の Process・プラグイン・ブローカーは直さなくても正しく動く(遅くなるだけで、今より遅くはならない)。
4. **同じ言語の受け取り側だけが、ネイティブ値をそのまま取り出す新しい API を使う。** 取り出せなければ(言語が違う、型が違う)、共通形式から decode する今の経路に落ちる。
5. **Composite 専用の経路は作らない。** Composite の中でも普通の接続でも、同じ仕組みで変換が消える(Composition 仕様 §2・§9)。

## 3. `Capsule` に足す形(juiz_sdk)

### 3.1 型

```rust
pub enum CapsuleValue {
    Empty(()),
    Value(Value),
    Image(DynamicImage),
    Native(NativeCell),          // 追加
}

/// ネイティブ値と、encode 済みのキャッシュ。Clone は Arc の複製(値は共有)
#[derive(Clone)]
pub struct NativeCell(Arc<NativeInner>);

struct NativeInner {
    value: Box<dyn NativeValue>,
    encoded: OnceLock<JuizResult<EncodedValue>>,   // encode の結果(失敗も覚える)
}

/// encode した結果。今の Capsule の中身の Value / Image と同じもの
pub enum EncodedValue { Value(Value), Image(DynamicImage) }

/// 言語ごとのネイティブ値が実装する。juiz_sdk は中身を知らない
pub trait NativeValue: Send + Sync + std::fmt::Debug {
    /// 作った言語の名前 (python / rust / cpp)
    fn language(&self) -> &str;
    /// encode したときの種類。encode せずに答えられなければならない (§5.2)
    fn kind(&self) -> NativeKind;              // Value | Image
    /// 表示用の型名 (trace・ログ・profile 用)。例: "numpy.ndarray"、"dict"
    fn type_name(&self) -> String;
    /// 共通形式を作る。失敗してよい
    fn encode(&self) -> JuizResult<EncodedValue>;
    /// 同じ言語の受け取り側が具体的な型へダウンキャストするため
    fn as_any(&self) -> &dyn std::any::Any;
    /// 型識別子 (§9)。Python は None
    fn type_id(&self) -> Option<&NativeTypeId> { None }
}
```

- `NativeValue` は `Send + Sync` でなければならない。`CapsulePtr` は `Arc<Mutex<Capsule>>` でスレッドをまたぐため。
- `NativeTypeId` は §9 で確定する。L1 では空の構造体として置いてよい(Python は使わない)。
- `Capsule` の `option`(`function_name` など)は今のまま `Native` とは別に持つ。

### 3.2 作り方と取り出し方(追加する API)

| API | 意味 |
|---|---|
| `Capsule::from_native(v: impl NativeValue + 'static) -> Capsule` | ネイティブ値を包む |
| `Capsule::is_native() -> bool` / `CapsulePtr::is_native() -> JuizResult<bool>` | 中身が Native か(encode しない) |
| `Capsule::as_native() -> Option<&dyn NativeValue>` | 中身のネイティブ値。同じ言語の受け取り側が `as_any().downcast_ref()` で具体的な型にする |
| `CapsulePtr::lock_as_native(func: FnOnce(&dyn NativeValue) -> T) -> JuizResult<T>` | ロックして中身のネイティブ値を渡す。Native でなければ `ValueTypeError` |
| `CapsulePtr::native_cell() -> JuizResult<Option<NativeCell>>` | ロックの外で使うために、中身の `NativeCell` を複製して返す(§6 の規則) |
| `Capsule::try_as_value() -> JuizResult<Option<&Value>>` | `as_value()` と同じだが、encode に失敗したらエラーを返す |
| `Capsule::try_as_image() -> JuizResult<Option<&DynamicImage>>` | 同上、Image 版 |

### 3.3 既存の API の振る舞い

| API | Native のとき |
|---|---|
| `is_empty()` | `false` |
| `is_value()` / `is_image()` | `NativeValue::kind()` で答える。**encode しない** |
| `as_value()` / `as_image()` | encode(初回だけ)して、キャッシュへの参照を返す。kind が違う、または encode に失敗したら `None`。失敗はログ(`log::error`)に出す |
| `as_value_mut()` | encode して、中身を `Value` に置き換えてから可変参照を返す(書き換えるとネイティブ値とずれるので、ネイティブ値を捨てる) |
| `to_value()` / `to_image()` | encode して取り出す。ほかに `NativeCell` の参照が無ければキャッシュを複製せずに移す |
| `CapsulePtr::lock_as_value()` など `lock_as_*` | §6 の規則で encode してから、今と同じに渡す。encode に失敗したらそのエラーを返す |
| `CapsulePtr::extract_value()` / `extract_image()` | 同上 |
| `TryInto<Value> for Capsule` / `capsule_to_value()` | encode した値で今の `{"__value__", "__option__"}` を作る |
| `Display` / `Debug` | `Native(<language>:<type_name>)`。encode しない |
| `Clone` | `NativeCell` の複製(値は共有)。ネイティブ値は複製しない |

## 4. Native を作る場所

- Native を作るのは**各言語のプラグインが Process の戻り値を返すとき**だけ。ブローカーから来た値、conf の既定値、p_apply の値は今のまま `Value` / `Image`。
- 戻り値が小さい値のときは Native にせず、今のまま encode してよい。L1(Python)では次のように決める。

| Python の戻り値 | 包み方 |
|---|---|
| `str`、`int`、`float`、`bool`、`None` | 今のまま `Value` にする(Native の方が高い。GIL を後で取る必要もなくなる) |
| `dict`、`list`、`tuple`、`set`、numpy の配列、PIL の画像、それ以外のオブジェクト | `Native(python)` にする |

- `kind()` は包むとき(GIL を持っている間)に決める。PIL の画像は `Image`、それ以外は `Value`。
- pyjuiz のデコレータが dataclass の戻り値を `dict` に直す処理(`dataclasses.asdict`)は L1 では変えない。dataclass のまま渡すのは #163 Step 5。

## 5. encode が起きる場所

encode は「共通形式を読む API」(§3.3)を呼んだときにだけ起きる。L1 の時点で、次の場所が共通形式を読む。

| 場所 | 今のコード | 振る舞い |
|---|---|---|
| ブローカー越しの応答(HTTP、qmp、IPC、messenger) | `capsule_to_value`、`capsule_ptr_to_response` | encode して今と同じ JSON / PNG を返す |
| 別の言語の Process への受け渡し(接続・Composite) | Rust の `TryInto` / `lock_as_value`、C++ の C ABI アクセサ、Haskell の C ABI | 受け取り側が共通形式を読むので encode が起きる |
| C ABI のアクセサ | `capsule_ptr_get_*`、`capsule_get_*`、`capsule_ptr_lock_as_value*` | encode して今と同じ値を返す。失敗はエラーコード |
| Topic への publish | `topics::capsule_ptr_to_capsule` | 今のまま `Value` に直して配る(L1 では Topic は最適化しない) |
| profile・管理画面 | `Inlet::current_value`(profile の引数ごとの今の値、issue #112) | encode して今と同じ値を見せる。管理画面が profile を読むたびに、Push 接続で inlet に入った Native 値が encode される(値ごとに 1 回)。L1 では変えず、重いときは後で「Native は型名だけ見せる」に変える |
| Composite の trace(Composition 仕様 §5.4) | `summarize` | **encode しない**。`native(<language>:<type_name>)` と出す |

### 5.1 encode しない場所

- 接続での受け渡し(outlet の memo、inlet、`push`)。`CapsulePtr` を複製するだけ。
- Composite のノードの間(`CompositeBody::eval`)。
- 同じ言語の受け取り側(§7)。

### 5.2 `kind()` を encode なしで答える理由

`is_value()` / `is_image()` はブローカーの応答の振り分け、Python の引数の変換などで、値を読む前に呼ばれる。ここで encode すると、言語が同じでも変換が起きてしまうため。

## 6. ロックと GIL の順序(必須)

ネイティブ値の encode は言語の実行環境の資源(Python なら GIL)を取る。一方、Python の Process は GIL を持ったまま引数の `CapsulePtr` をロックする。`CapsulePtr` のロックを持ったまま GIL を取ると、逆順で待つスレッドと相互待ちになる。そのため次を守らなければならない。

1. **encode は `CapsulePtr` の `Mutex` を持たずに行う。** `lock_as_value` などは、ロックして `NativeCell` を複製 → ロックを放す → encode(`OnceLock` に入れる)→ もう一度ロックしてキャッシュを読む、の順にする。
2. キャッシュは `NativeCell` の中(`OnceLock`)に持つので、`Capsule` のロックなしで書ける。同じ値を 2 つのスレッドが同時に encode しようとしても、`OnceLock` が 1 回にまとめる。
3. 2 回目のロックの間に中身が差し替わっていたら(`CapsulePtr::replace`)、差し替わった後の中身で最初からやり直す。
4. `Capsule`(生ポインタ、C ABI の `capsule_*`)の `as_value()` は `CapsulePtr` のロックの外で呼ばれるので、その場で encode してよい。
5. `NativeValue::drop` は GIL なしで呼ばれてもよくなければならない。pyo3 の `Py<T>` は GIL が無いときは参照の減算を後回しにするので、そのままでよい。

## 7. 受け取り側(同じ言語)

- 受け取り側のプラグインは、引数の `CapsulePtr` が Native で、`language()` が自分の言語で、`as_any()` で自分の型にダウンキャストできれば、**ネイティブ値をそのまま使う**。そうでなければ今の decode を使う。
- L1(Python)では、python_plugin の `capsuleptr_to_pyany` が `Native(python)` を見つけたら、包んでいる `PyObject` の参照を増やして(`clone_ref`)そのまま引数にする。
- 言語が同じでも、型識別子(§9)が合わないときはネイティブ値を使ってはならない(Rust / C++)。Python は型識別子を使わず、常にそのまま渡す(Python の関数呼び出しと同じ意味)。

### 7.1 共有と書き換え(Python)

- ネイティブ値は共有される。outlet の memo、複数の接続先、Composite の親が同じ `PyObject` を見る。
- そのため、**受け取った引数をその場で書き換える(`dict` への代入、numpy の `+=` など)と、memo やほかの接続先にも見える**。今は受け渡しのたびに複製されていたので、これは振る舞いの変化になる。
- 規則: Python の Process は引数を書き換えてはならない(書き換えたいときは複製する)。これは Python の関数呼び出しで普通に守る約束と同じなので、juiz では検査しない。ガイド(C9)とサンプルに書く。

## 8. 所有権(Rust / C++ 向け、L2・L3 で使う)

- Composite の中間値は受け取り手が 1 つ(Composition 仕様 §2 の木構造、§5.3 の「中間値は memo しない」)なので、`NativeCell` の参照も 1 つになる。受け取り側は `Arc::try_unwrap` で値を**移して**よい(clone 不要)。
- 普通の接続では memo と複数の接続先が共有するので、移せない。受け取り側は参照で読むか、codec の `clone` で複製する。
- Rust の Process の引数は値渡し(`A`)で受け、移せなければ `clone` する(計画書 §9 の論点 11)。
- codec の表(`encode` / `decode` / `destroy` / `clone` / 型識別子)を C ABI で登録する形は L3 で決め、登録所は #163 Step 4 の型の登録所と同じものにする。

## 9. 型識別子と照合(未確定、#163 Step 0 の後)

ここは #163 Step 0(Avro の指紋と互換性の決まり)が決まってから確定する。今の決めごとは次のとおり。

- 型識別子 = 完全名(例 `robot.geometry.Pose2D`)+ 構造のハッシュ(#163 の Avro の Parsing Canonical Form の指紋)+ ABI タグ(コンパイラ・SDK の版)。自前のハッシュは作らない。
- 照合は 3 段階:
  1. 指紋と ABI タグが同じ → ネイティブ値のまま渡す。
  2. 指紋は違うが互換(フィールドの追加・既定値あり など、#163 の規則) → encode → decode(codec)を通す。
  3. 互換でない → 定義時・接続時にエラー(Composite は C8、接続は #163 Step 4)。
- `any`(型なし)の値は Native にしない。常に `Value` で流す。
- Rust は最初は**同じプラグイン(cdylib)の中だけ**ネイティブ値を渡す(別プラグインは `repr(Rust)` の並びと `TypeId` が一致する保証がないため)。

## 10. プラグインを外すとき

- encode の関数(`NativeValue` の vtable)はプラグインの中にある。プラグインの Native 値が残ったままプラグインを外すと危ない。
- 今の juiz は実行中にプラグインを外さないので、**当面は「外さない」前提**とする。外す機能を作るときは、先にそのプラグインが作った Native 値をすべて encode して `Value` / `Image` に置き換える。
- Python の Native 値は juiz_core の python_plugin(インタプリタは juiz の終了まで残る)の中なので、この問題はない。

## 11. 互換性

- 変えないもの: `Process` トレイト、`ProcessImpl`、接続の仕組み、通信形式(JSON / MessagePack の中身)、ID の形式、既存の C ABI 関数の意味、conf。
- 足すもの: `CapsuleValue::Native`、`NativeCell`、`NativeValue` トレイト、`NativeKind`、`EncodedValue`、§3.2 の API。
- `CapsuleValue` に列挙子が増えるので、juiz の外で `CapsuleValue` を `match` しているコードは対応が要る(juiz の中は対応する)。
- Python の Process の引数の書き換えが共有先に見えるようになる(§7.1)。
- 今は `todo!()` で落ちている numpy の配列の戻り値(`python_process_call`)は、Native になるので Python 同士なら動くようになる。Python 以外が読むときの encode は `ndarray.tolist()` の `Value` にする(画像としては扱わない)。

## 12. Step との対応

| Step | issue | この文書の範囲 |
|---|---|---|
| L0 仕様 | #174 | この文書。§9 は #163 Step 0 の後に確定し、そのとき L0 を閉じる |
| L1 Python | #175 | §3〜§7・§10・§11。juiz_sdk の `CapsuleValue::Native` と §3.2 の API、§6 のロックの順序、python_plugin の §4 と §7。テストは Python → Python(接続と Composite)で encode が起きないこと、Python → Rust・ブローカー越し・Topic で今と同じ値になること、numpy の配列を Python 同士で渡せること。`juiz_core/tests/composite_bench.rs` で効果を測り、結果を #175 に残す |
| L2 Rust | #176(#163 Step 3 とまとめる) | §8・§9。`#[juiz_process]` 系マクロの戻り値を任意の型にして Native で返す |
| L3 C++ | #177(#163 Step 6 とまとめる) | §8・§9。`JUIZ_TYPE` による codec 登録、Native を読み書きする C ABI 関数の追加 |

## 13. 対象外(後の Step か、未定)

- Haskell の遅延評価(`StablePtr` と `ToValue / FromValue`)
- Topic の最適化(Native のまま配る)
- ブローカー越し・別プロセスでの省略(必ず encode する)
- 実行中にプラグインを外す機能(§10)
- 引数の書き換えの検査(§7.1)
