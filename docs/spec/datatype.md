# データ型(IDL・データモデル・指紋・互換性)仕様

- 状態: 確定(Step 0、issue #189、2026-10-26)
- 親 issue: #163。計画と経緯はプロジェクトの計画書 datatype-plan.md(2026-10-26 確定)にある
- この文書は #163 の Step 1〜13(#190〜#201)と、遅延評価(#3)の型識別子(docs/spec/lazy-evaluation.md §9)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と用語

Process の引数と戻り値に**名前のついたデータ型**(`robot.geometry.Pose2D` など)を付けられるようにし、型定義ファイル(IDL)から各言語のコードを生成できるようにする。型は**契約**であり、通信で流れるデータの形は今の辞書(JSON 相当の値)のまま変えない。

| 用語 | 意味 |
|---|---|
| スキーマ | 1 つの型の定義。Apache Avro 1.11 のスキーマ(.avsc の JSON)で表す。juiz の型の正本 |
| 名前付き型 | `record` と `enum`。完全名(名前空間 + 名前)を持つ |
| 完全名 | `robot.geometry.Pose2D` のような、名前空間と名前を `.` でつないだ名前 |
| 型の名前空間 | 名前付き型の名前空間(`robot.geometry`)。**ID ネームスペース(`core://core/process/<ns>/<name>::<type>` の `<ns>`)とは別物** |
| JIDL | juiz の IDL。Avro IDL に寄せた文法の、人が書く型定義ファイル(拡張子 `.jidl`)。生成器 `juiz-idl` がスキーマに変換する |
| 指紋 | スキーマの Parsing Canonical Form(PCF)の SHA-256(§5) |
| 型参照 | manifest に載せる、1 つの引数・戻り値の型の情報(§6) |
| 型付き / 型なし | 型参照を持つ引数・戻り値を型付き、持たないもの(今の `ArgumentType` だけのもの)を型なしと呼ぶ |
| writer / reader | 値を作る側の型 / 受け取る側の型。Avro のスキーマ解決の用語をそのまま使う |
| 互換 | writer の型の値を reader の型として読めること(§7) |

## 2. 基本方針

1. **型は契約、データ表現は今のまま。** 通信(HTTP / QMP / IPC / Messenger)は今の JSON / MessagePack の辞書を流す。Avro のバイナリ形式も Avro の JSON 形式も使わない(§4)。
2. **型付けは任意(opt-in)。** 型なしの Process・引数は今のまま動く。型付きと型なしはいつでもつながる。
3. **スキーマが正本。** 型の同一性・互換性・指紋は Avro の仕様に従って、スキーマ(.avsc)から決める。JIDL は書きやすくするための入口で、`#[derive(JuizType)]`(Rust)も同じスキーマを出す。
4. **C ABI は変えない。** 型の情報は manifest(JSON)に載せ、値は今の `value` で受け渡す。
5. **接続時は厳しく、呼び出し時は緩く。** 接続(Connection・Topic・Composite の式の木)で互換でない型をつないだらエラー。呼び出し時の値の検査は既定でオフ(§8)。

## 3. データモデル

### 3.1 使える型

Avro 1.11 の型のうち、次のものを使う。ここにない Avro の型・機能(`fixed`、一般の union、`error`、logical type など)はスキーマに書いたらエラーにする(§3.4)。

| 種類 | Avro の書き方 | JIDL の書き方 | 内容 |
|---|---|---|---|
| null | `"null"` | `null` | 値がないこと。単独で引数の型にはできない。戻り値には使える(JIDL の `void` と同じ) |
| 真偽 | `"boolean"` | `boolean` | |
| 32 bit 整数 | `"int"` | `int` | |
| 64 bit 整数 | `"long"` | `long` | juiz の既定の整数型 |
| 32 bit 浮動小数点 | `"float"` | `float` | |
| 64 bit 浮動小数点 | `"double"` | `double` | juiz の既定の浮動小数点型 |
| 文字列 | `"string"` | `string` | UTF-8 |
| バイト列 | `"bytes"` | `bytes` | |
| 配列 | `{"type":"array","items":T}` | `array<T>` | |
| 辞書 | `{"type":"map","values":T}` | `map<T>` | 鍵は文字列だけ(Avro の規則) |
| 省略可能 | `["null",T]` または `[T,"null"]` | `T?` または `union { null, T }` | **union は null と 1 つの型の組だけ**を使える |
| レコード | `{"type":"record","name":...,"fields":[...]}` | `record Name { ... }` | 名前付き型 |
| 列挙 | `{"type":"enum","name":...,"symbols":[...]}` | `enum Name { A, B } = A;` | 名前付き型。値は記号の文字列 |
| 任意の値 | `{"type":"record","name":"Any","namespace":"juiz","fields":[]}` | `any` | juiz の拡張。今の辞書と同じく、何でも入る(§3.2) |
| 画像 | `{"type":"record","name":"Image","namespace":"juiz","fields":[]}` | `image` | juiz の拡張。`Capsule::Image`(§3.3) |

- 整数の範囲: `int` は -2^31〜2^31-1、`long` は -2^63〜2^63-1。符号なし 64 bit(`u64`)の型は持たない(§10)。
- レコードのフィールド名、列挙の記号、名前付き型の名前は Avro の名前の規則(`[A-Za-z_][A-Za-z0-9_]*`)に従わなければならない。型付きの Process の引数名も同じ規則に従う(型なしの引数は今の規則のまま)。
- フィールドの順番はスキーマの一部で、指紋に入る(§5)。ただし辞書の上ではキーの順番に意味はない(§4)。

### 3.2 `any`

- `any` は今の `ArgumentType::Object` などと同じく、JSON 相当の値なら何でも入る。型付きの型の中に「ここは自由」を残す逃げ道で、レコードのフィールドや配列の要素にも使える。
- スキーマ上は名前空間 `juiz` の予約された空のレコード `juiz.Any` で表す。Avro の PCF は `logicalType` などの属性を落とすので、属性ではなく完全名で区別する(指紋が他の型とぶつからないようにするため)。
- 予約名 `juiz.Any` / `juiz.Image` は、juiz 以外の Avro ツールには空のレコードに見える。juiz の検査器・生成器は完全名で特別扱いしなければならない。
- `any` は値を検査しない。互換性は §7.2 のとおり、何とでも互換。

### 3.3 `image`

- `image` は `Capsule::Image`(`DynamicImage`)を表す。Value(辞書)の外にある値なので、**引数か戻り値の型そのものとしてだけ**使える。レコードのフィールド、配列・辞書の要素、`image?` に使ったらエラーにする。
- スキーマ上は予約された空のレコード `juiz.Image` で表す。
- 通信は今のまま(HTTP は PNG、QMP は今の Image の扱い)。

### 3.4 使えないもの

- `fixed`、`error`、union(null と 1 つの型の組以外)、logical type(`decimal`、`uuid`、`date`、`timestamp-*` など)、名前空間 `juiz` の下の予約名以外の名前、再帰する型(自分自身を直接・間接にフィールドに持つレコード)はエラーにする。
- logical type と一般の union は、辞書の上での表し方を決めてから後で足してよい(§14)。足すまでは、スキーマに `logicalType` が書いてあれば下の型(`long` など)として扱わず、エラーにする。

## 4. 辞書での値の表し方

通信と各言語の型なしの API では、型付きの値も今の辞書(`serde_json::Value` 相当)で表す。**Avro の JSON エンコーディングは使わない**(union を `{"string": "abc"}` のように型名で包むので、今の辞書と互換でない)。

| 型 | 辞書での値 | Avro の JSON エンコーディングとの違い |
|---|---|---|
| `null` | `null` | 同じ |
| `boolean` | `true` / `false` | 同じ |
| `int`, `long` | JSON の整数。範囲外はエラー。`1.0` のような小数表記は受け付けない | 同じ |
| `float`, `double` | JSON の数(整数でも小数でもよい) | 同じ。`NaN`・`Infinity` は辞書に入らない(JSON に表せない) |
| `string` | JSON の文字列 | 同じ |
| `bytes` | **base64(RFC 4648 §4、パディングあり)の文字列** | Avro は文字コード 0〜255 の文字列で表す |
| `array<T>` | JSON の配列 | 同じ |
| `map<T>` | JSON のオブジェクト | 同じ |
| `T?` | `null`、または T の値を**そのまま** | Avro は `{"T の名前": 値}` で包む |
| record | JSON のオブジェクト(フィールド名 → 値)。キーの順番は問わない | 同じ |
| enum | 記号の文字列 | 同じ |
| `any` | JSON の値なら何でも | (Avro にない) |
| `image` | 辞書には入らない。`Capsule::Image` で渡す | (Avro にない) |

レコードの読み方(reader の型で辞書を読むとき。生成コードの `from_value` と呼び出し時の検査で同じ):

- reader のフィールドが辞書になければ、そのフィールドの既定値を使う。既定値もなければエラー。`T?` のフィールドで既定値がなく、キーもないときもエラー(`null` を明示する)。
- 辞書に reader のフィールドにないキーがあれば無視する(エラーにしない)。writer の型が新しくてフィールドが増えている場合に読めるようにするため。
- 生成コードの `to_value` は、すべてのフィールドを出す(既定値と同じ値でも省かない)。`T?` が空なら `null` を出す。

MessagePack(QMP)は今のとおり Value を変換したものを流すので、上の表と同じになる(`bytes` も base64 の文字列)。Avro のバイナリ形式で QMP を速くするのは将来の検討(§14)。

## 5. 型の識別子と指紋

### 5.1 指紋

- 指紋は、型のスキーマを Avro 1.11 の **Parsing Canonical Form(PCF)** にした UTF-8 のバイト列の **SHA-256** とする。CRC-64-AVRO(Rabin)は使わない。
  - 理由: 型は別々の人・リポジトリで独立に定義され、登録所や遅延評価で同一性の判定に使うので、64 bit では衝突の心配が残る。ROS 2 の型ハッシュ(RIHS01)も SHA-256 で、Step 13 で並べて扱いやすい。計算は登録時・接続時だけなので重さは問題にならない。
- 文字列での書き方は `sha256:` + 64 桁の小文字 16 進数。表示用の短い形は先頭 8 桁(例 `Pose2D@1a2b3c4d`)とし、短い形を同一性の判定に使ってはならない。
- PCF は名前付き型を最初に出てきた場所で展開し、2 回目以降は完全名で参照する。指紋は**参照している型もすべて含めた**スキーマから計算する(`Pose2D` の指紋は `Point2D` の中身が変われば変わる)。
- PCF は `doc`、`aliases`、`default`、`order`、`logicalType` などの属性を落とす。したがって**既定値・説明・別名を変えても指紋は変わらない**。フィールドの型・名前・順番、記号の並び、名前付き型の完全名を変えると指紋が変わる。
- 名前のない型(`long`、`array<robot.geometry.Pose2D>` など)にも同じ方法で指紋がある。
- Rust の実装は apache-avro クレートの `Schema::canonical_form()` と SHA-256 を使ってよい。

### 5.2 型の識別子

- 名前付き型の識別子は **完全名 + 指紋** とする。どちらか一方だけで同一とみなしてはならない(同じ完全名で中身が違う型や、同じ中身で名前が違う型がありうるため)。
- 文字列での書き方は `<完全名>@sha256:<64 桁>`(例 `robot.geometry.Pose2D@sha256:1a2b…`)。
- 名前のない型の表示名は JIDL の書き方(`array<robot.geometry.Pose2D>`、`long?` など)とする。
- 型パッケージの版(crate や wheel の版)は識別子に入れない。版が違っても指紋が同じなら同じ型として扱う。

## 6. manifest での型の表し方

型付きの引数・戻り値は、manifest に**型参照**を載せる。型なしの引数・戻り値は今のまま(型参照を持たない)。

```json
{
  "type_name": "Object",
  "name": "target",
  "description": "",
  "default": { "position": { "x": 0.0, "y": 0.0 }, "theta": 0.0, "frame": "map" },
  "schema": {
    "name": "robot.geometry.Pose2D",
    "fingerprint": "sha256:1a2b…",
    "avro": { "type": "record", "name": "Pose2D", "namespace": "robot.geometry", "fields": [ … ] }
  }
}
```

- `ArgumentManifest` に `schema`(省略可能)を足す。戻り値は `ProcessManifest` に `output_schema`(省略可能)を足し、形は同じにする。Step 1 で SDK に `TypeSchema`(この JSON の型)を足す。
- `schema.name` は §5.2 の表示名(名前付き型は完全名)。`schema.fingerprint` は §5.1 の指紋。`schema.avro` は**それだけで完結した**スキーマ(参照する名前付き型を最初に出てきた場所で全部展開したもの。`doc` と `default` と `aliases` は残す)。接続時の互換性の判定に登録所を引かなくて済むようにするため。
- `type_name`(今の `ArgumentType`)は残し、型から次のように決める。古いクライアントは `schema` を無視すればよい。

| 型 | `type_name` |
|---|---|
| `boolean` | `Bool` |
| `int`, `long` | `Int` |
| `float`, `double` | `Float` |
| `string`, `bytes`, enum | `String` |
| `array<T>` | `Array` |
| `map<T>`, record, `any` | `Object` |
| `image` | `Image` |
| `T?` | T の `type_name` |

- 型付きの引数の `default` は、§4 の辞書での表し方で書く。manifest を作るときに §4 の規則で検査し、合わなければエラー(今の `new_with_check` と同じ扱い)。
- 今の `outputs`(`OutputProfile`、pyjuiz だけが埋めている)は互換のために残す。新しいコードは `output_schema` を使う。Rust の SDK は `output_schema` を付けるとき、`outputs` が空なら `type_name` だけの `outputs`(`{"enum_type": "Primitive", "type_name": "Object", "default_value": null}` など)も埋める(古いクライアントの表示用。Step 1)。
- 型付きの引数の `default` が `null` で、型が `T?` でなければ「既定値なし」として扱い、検査しない(今の Image 引数などと同じ)。`image` の引数の `default` は検査しない。
- 戻り値が `void`(`null`)の型付きの Process は、今の `Capsule::Empty` を返してよい。

## 7. 互換性

### 7.1 判定の結果

型を比べる関数(Step 1 で SDK に置く。接続時の検査 Step 4、Composite の C8 #172、遅延評価 L1〜L3 が使う)は、writer と reader の型参照を受け取り、次の 3 つのどれかを返す。

| 結果 | 条件 |
|---|---|
| 同じ | 完全名(名前のない型は表示名)と指紋がどちらも一致する |
| 互換 | 同じではないが、§7.2 の規則で writer の値を reader として読める |
| 互換でない | それ以外。どこが合わないか(型の中の位置 `Pose2D.position.x` など)と理由を付ける |

一方か両方が型なしのときは、判定せず「型なし」として扱い、今のとおりつなぐ(§8)。

### 7.2 規則

Avro 1.11 のスキーマ解決(Schema Resolution)の規則のうち、§4 の辞書の表し方で成り立つものを使う。writer を W、reader を R とする。

1. **`any`**: W か R が `any` なら互換(W が `image` のときを除く)。
2. **`image`**: `image` と互換なのは `image` と `any`(R が `any` のとき)だけ。
3. **同じ基本型**は互換。
4. **数の昇格**: `int` → `long` / `float` / `double`、`long` → `float` / `double`、`float` → `double` は互換(Avro と同じ)。逆向きは互換でない。
5. **`string` と `bytes` は互換でない**(Avro は互換とするが、辞書では `bytes` が base64 なので読み違える)。
6. **配列・辞書**: 要素(`items` / `values`)が互換なら互換。
7. **省略可能**:
   - W が `T`、R が `U?` → `T` と `U` が互換なら互換。
   - W が `T?`、R が `U?` → `T` と `U` が互換なら互換。
   - W が `T?`、R が `U`(省略不可) → **互換でない**(`null` が来たら読めないため)。
   - W が `null`、R が `U?` → 互換。
8. **レコード**: 次をすべて満たすとき互換。
   - R の完全名が W の完全名と同じか、R の `aliases` に W の完全名がある。
   - R の各フィールドについて、W に同じ名前(または R のフィールドの `aliases` の名前)のフィールドがあれば、その型どうしが互換。なければ R のフィールドに既定値がある。
   - W にあって R にないフィールドは無視する(互換のまま)。
9. **列挙**: R の完全名が W の完全名と同じか `aliases` にあり、かつ W の記号がすべて R の記号にあるか、R に列挙の既定値(`default`)があるとき互換。
10. 上のどれにも当たらなければ互換でない。完全名が違うレコード・列挙は、中身が同じでも互換でない(`aliases` で明示する)。

- 制約の属性(§10 の `juiz.maxLength` など)は互換性の判定に使わない。
- apache-avro クレートの `SchemaCompatibility` は規則 5 と予約名の扱いが違うので、そのままは使わない。下請けに使う場合は、juiz の規則を先に当てはめなければならない。

### 7.3 型を変えるときの目安

型パッケージの作者向けの目安(公式サイトのガイドに載せる)。「新 → 旧」は新しい型の値を古い型で読めるか、「旧 → 新」はその逆。

| 変更 | 新 → 旧 | 旧 → 新 | 指紋 |
|---|---|---|---|
| 既定値つきのフィールドを足す | ○ | ○ | 変わる |
| 既定値なしのフィールドを足す | ○ | × | 変わる |
| 既定値つきのフィールドを消す | ○ | ○ | 変わる |
| フィールドの名前を変え、新しい名前の `aliases` に古い名前を書く | 旧のフィールドに既定値があれば ○ | ○ | 変わる |
| フィールドの型を `int` → `long` | × | ○ | 変わる |
| フィールドの順番を変える | ○ | ○ | 変わる |
| 列挙に記号を足す | 旧に既定値があれば ○ | ○ | 変わる |
| 既定値・説明だけ変える | ○ | ○ | 変わらない |
| 型の名前空間・名前を変える | `aliases` があれば ○ | `aliases` があれば ○ | 変わる |

列挙には既定値を付けておくことを勧める(記号を足しても古い側が読める)。

## 8. 検査の時機

| 時機 | 内容 | 既定 |
|---|---|---|
| manifest を作るとき | 型付きの引数の `default` を §4 で検査する。スキーマが §3 に従わなければエラー | 常に行う |
| 接続時 | Connection を作るとき、source の `output_schema`(W)と destination の引数の `schema`(R)を §7 で比べる。互換でなければ接続しないでエラー | 常に行う |
| Topic | Process が publish / subscribe を始めるとき、その Topic の既存の相手それぞれと比べる(publish 側が W、subscribe 側が R)。互換でない組ができるならエラーにして、始めない | 常に行う |
| Composite の定義時 | 子の戻り値(W)と親の引数(R)を比べる(C8 #172)。`const` と `expose` の `default` は §4 で検査する | 常に行う |
| 呼び出し時 | ブローカー越しの `call` / `execute` の入口で、引数の辞書を §4 の規則で引数の型と照らし合わせ、合わなければ関数を呼ばずにエラー | **オフ**。conf の `option.type_check.on_call` で入れる(§19.3) |

- 型なしの相手とは、どの時機でも検査しない(今のまま)。
- 型付きの Process を、型を知らないクライアント(CLI、curl、古い pyjuiz)が辞書で呼んでもよい。呼び出し時の検査がオフでも、生成コードの `from_value` が §4 の規則で読み、読めなければ型変換のエラーになる。
- エラーの種類は Step 1 で `JuizError` に足す。名前は `TypeSchemaInvalidError`(スキーマや JIDL が規則に合わない)、`TypeIncompatibleError { writer, reader, path, reason }`(互換でない)、`ValueDoesNotMatchTypeError { type_name, path, reason }`(辞書が型に合わない)とする。

## 9. 書く IDL(JIDL)

### 9.1 例

```text
// geometry.jidl
namespace robot.geometry;

/** 平面上の点 */
record Point2D {
  double x;
  double y;
}

record Pose2D {
  Point2D position;
  double theta = 0.0;
  string frame = "map";
  @aliases(["note"]) string? comment = null;
}

enum MotorMode { Idle, Velocity, Position } = Idle;

/** 指定の姿勢まで動く */
process Pose2D move_to(Pose2D target, double speed = 1.0);

process image capture_gray(image img);

container motor {
  container_process boolean set_mode(MotorMode mode);
  container_process void stop();
}
```

### 9.2 文法

Avro IDL 1.11 の schema 構文に寄せ、`protocol` の代わりに `process` / `container` 宣言を足す。

```ebnf
file          = namespace_decl , { import_decl } , { declaration } ;
namespace_decl= "namespace" , qualified_name , ";" ;
import_decl   = "import" , ( "idl" | "schema" ) , string_literal , ";" ;
declaration   = { annotation } , [ doc_comment ] ,
                ( record_decl | enum_decl | process_decl | container_decl ) ;
record_decl   = "record" , name , "{" , { field_decl } , "}" ;
field_decl    = [ doc_comment ] , { annotation } , type , name , [ "=" , json_value ] , ";" ;
enum_decl     = "enum" , name , "{" , name , { "," , name } , "}" , [ "=" , name ] , ";" ;
process_decl  = "process" , return_type , name , "(" , [ params ] , ")" , ";" ;
container_decl= "container" , container_type_name , "{" , { [ doc_comment ] , container_process_decl } , "}" ;
container_process_decl = "container_process" , return_type , name , "(" , [ params ] , ")" , ";" ;
params        = param , { "," , param } ;
param         = [ doc_comment ] , type , name , [ "=" , json_value ] ;
return_type   = type | "void" ;
type          = base_type , [ "?" ] ;
base_type     = "boolean" | "int" | "long" | "float" | "double" | "string" | "bytes" | "null"
              | "any" | "image"
              | "array" , "<" , type , ">" | "map" , "<" , type , ">"
              | "union" , "{" , "null" , "," , type , "}" | "union" , "{" , type , "," , "null" , "}"
              | qualified_name ;
annotation    = "@" , ( "aliases" | "namespace" ) , "(" , json_value , ")" ;
```

- コメントは `//`(行末まで)と `/* … */`。`/** … */` は直後の宣言・フィールド・引数の `doc` になる。
- `name` は Avro の名前の規則。キーワードと同じ名前はバッククォートで囲む(`` `record` ``)。`qualified_name` は `.` 区切り。
- `namespace` はファイルに 1 つ、先頭に必須。名前空間 `juiz` とその下(`juiz.*`)は予約で、使ったらエラー。
- 型の名前は、同じファイルの名前空間、次に完全名の順で解決する。他のファイルの型は `import idl "other.jidl";`、手で書いた .avsc は `import schema "x.avsc";` で取り込む。
- 既定値 `json_value` は §4 の辞書の表し方で書く(`bytes` は base64 の文字列)。生成器が .avsc に書き出すときは Avro の既定値の規則(`bytes` は 0〜255 の文字列)に変換する。
- `T?` は、既定値がないか `null` なら `["null",T]`、`null` 以外なら `[T,"null"]` に変換する(Avro は union の既定値を最初の型で書く決まりのため。Avro IDL と同じ)。
- `@aliases([...])` は record・enum・フィールドの `aliases`。`@namespace("…")` はその型だけ名前空間を変える。それ以外の注釈はエラー。
- 宣言の順番は自由で、後ろの型を前で参照してよい(再帰は §3.4 でエラー)。

### 9.3 `process` / `container` 宣言

- `process` 宣言は ProcessFactory の**シグネチャ**(引数名・型・既定値・戻り値の型・説明)を決める。`type_name` は宣言の名前と同じ。インスタンス名・ブローカー・接続は今どおり conf で決める。
- `container <container_type_name> { … }` の中の `container_process` 宣言は、その Container の型に属する ContainerProcessFactory のシグネチャ。Container 自体の状態は型付けしない。`container_type_name` と process の名前は `validate_name` の規則にも従う。
- 引数の並びは manifest の `arguments` の順番になる(Composite の評価順に使われる)。
- `image` は引数・戻り値の型そのものとしてだけ使える(§3.3)。
- `process` / `container` 宣言は Avro の型ではないので、指紋は持たない。生成器は型ごとの .avsc とは別に、シグネチャを `<namespace>.processes.json` に書き出す。

```json
{
  "namespace": "robot.geometry",
  "processes": [
    { "type_name": "move_to", "description": "指定の姿勢まで動く",
      "arguments": [ { "name": "target", "type_name": "Object", "schema": { … §6 … } },
                     { "name": "speed", "type_name": "Float", "schema": { … }, "default": 1.0 } ],
      "output_schema": { … } }
  ],
  "containers": [
    { "type_name": "motor",
      "container_processes": [ { "type_name": "set_mode", "arguments": [ … ], "output_schema": { … } } ] }
  ]
}
```

### 9.4 .avsc を直接書く

- JIDL を使わず .avsc を直接書いてもよい。§3 の規則を満たさなければならない。
- `juiz-idl` は .avsc を JIDL と同じく入力に取り、各言語のコードを生成できる(`process` 宣言はないので型だけ)。

## 10. ROS 2 の型の取り込みの方針(実装は Step 13 #201)

ROS 2 の `.msg` / `.srv` / `.idl` を JIDL / .avsc に変換する。juiz の型にない性質は、**型は近いものに寄せ、制約は指紋に入らない属性として残す**。属性は §8 の呼び出し時の検査でだけ使い、互換性の判定には使わない(§7.2)。

| ROS 2 | juiz | 残す属性 |
|---|---|---|
| `bool` | `boolean` | |
| `byte`, `char`, `int8`, `uint8`, `int16`, `uint16`, `int32` | `int` | `juiz.ros_type`(元の型名)、範囲 |
| `uint32`, `int64` | `long` | `juiz.ros_type` |
| `uint64` | `long` | `juiz.ros_type`。2^63 以上の値は表せないので、取り込み時に警告し、実行時は範囲外エラー |
| `float32` / `float64` | `float` / `double` | |
| `string`, `wstring` | `string` | |
| `string<=N` | `string` | `juiz.maxLength: N` |
| `T[]` | `array<T>` | |
| `T[N]`(固定長配列) | `array<T>` | `juiz.minItems: N`, `juiz.maxItems: N` |
| `T[<=N]`(長さ制限つき配列) | `array<T>` | `juiz.maxItems: N` |
| `uint8[]`, `byte[]` | `bytes` | |
| 型 `pkg/msg/Name` | record `ros2.pkg.msg.Name` | |
| フィールドの既定値 | 既定値 | |
| 定数(`int32 FOO=1`) | 型には入れない | record の属性 `juiz.constants` に残し、生成コードの定数にする |
| `.srv` | `process` 宣言(Request のフィールド → 引数、Response → record `<Name>_Response` を戻り値) | |
| `.action` | 対象外 | |

- 名前空間 `ros2.*` は取り込み用に予約する(人が書く JIDL で使ってもよいが勧めない)。
- `sensor_msgs/msg/Image` などは record として取り込み、juiz の `image` には自動で変換しない(変換用の Process は別に用意してよい)。
- 書き出し(juiz → ROS 2)は後で検討する。

## 11. 言語との対応

各言語の生成コード(Step 2・5・6・7)の型の対応の基準。細部は各 Step で決め、ここを直す。

| juiz | Rust | Python | C++ | Haskell | TypeScript |
|---|---|---|---|---|---|
| `boolean` | `bool` | `bool` | `bool` | `Bool` | `boolean` |
| `int` / `long` | `i32` / `i64` | `int` | `int32_t` / `int64_t` | `Int32` / `Int64` | `number` |
| `float` / `double` | `f32` / `f64` | `float` | `float` / `double` | `Float` / `Double` | `number` |
| `string` | `String` | `str` | `std::string` | `Text` | `string` |
| `bytes` | `Vec<u8>` | `bytes` | `std::vector<uint8_t>` | `ByteString` | `string`(base64) |
| `array<T>` | `Vec<T>` | `list[T]` | `std::vector<T>` | `[T]` | `T[]` |
| `map<T>` | `HashMap<String, T>` | `dict[str, T]` | `std::map<std::string, T>` | `Map Text T` | `Record<string, T>` |
| `T?` | `Option<T>` | `T \| None` | `std::optional<T>` | `Maybe T` | `T \| null` |
| record | `struct` | `@dataclass` | `struct` | record | `interface` |
| enum | `enum` | `enum.Enum`(値は記号の文字列) | `enum class` | 直和型 | 文字列リテラルの union |
| `any` | `Value` | `Any` | `juiz::value` | `Value` | `unknown` |
| `image` | `DynamicImage` | `PIL.Image.Image` | `juiz::Image`(large-data.md §10) | `Image`(`Juiz.Image`、large-data.md §10) | (扱わない) |

- 生成コードは型ごとに、スキーマ(§6 の `TypeSchema`)を返す関数と、§4 の辞書との変換(`to_value` / `from_value` に当たるもの)を持つ。
- Rust の `#[derive(JuizType)]` は、Rust の struct / enum から同じスキーマを作る。名前空間は `#[juiz(namespace = "robot.geometry")]` で書き、省略時はエラー。フィールドの順番は宣言順。JIDL で同じ形に書いた型と指紋が一致しなければならない。derive は Rust の中で閉じ、他の言語へは JIDL(または derive が書き出す .avsc)経由で渡す。
- pyjuiz で生成コードを使わない今の dataclass は、今のとおり型なし(`Object`)のまま扱う。生成コードの型を使ったときだけ型付きになる(§20)。

## 12. 遅延評価(#3)の型識別子

遅延評価(docs/spec/lazy-evaluation.md §9)は、ネイティブ値を渡してよいかの判定に次の**型タグ**を使う。

- 型タグ = 完全名 + 指紋(§5)+ **ABI タグ**。
- ABI タグは、同じメモリ表現であることを保証する範囲を表す文字列で、**完全一致だけ**で比べる(中身を解釈しない)。言語ごとの中身は次を基準にし、L1〜L3 で決める。
  - Rust: `rust:<rustc の版>:<cdylib を一意に表すもの>`(L0 §9 のとおり、最初は同じプラグインの中だけ。形は §17.1)
  - Python: `python:<インタプリタを一意に表すもの>`(同じ juiz プロセスの同じインタプリタの中だけ)
  - C++: `cpp:<コンパイラと標準ライブラリの版>:<型パッケージの版>`
- 照合の 3 段階:
  1. 完全名・指紋・ABI タグがすべて同じ → ネイティブ値のまま渡す。
  2. それ以外で、§7 で互換 → encode(§4 の辞書)→ decode を通す。
  3. 互換でない → 定義時・接続時にエラー(§8)。
- `any` と型なしの値は Native にしない。ただし Python は同じインタプリタの中なので、型なしの値も Native にしてそのまま渡す(L0 §9.3)。

## 13. Step との対応

| Step | issue | この文書の節 |
|---|---|---|
| 1 SDK | #190 | §3、§4、§5、§6、§7、§8 のエラー、§11 の `#[derive(JuizType)]`(§15) |
| 2 生成器 `juiz-idl` | #191 | §5、§9、§11(Rust、JSON Schema)(§16) |
| 3 Rust マクロ | #192 | §6、§11(`#[juiz_process]` 系が `JuizType` を受ける)(§17) |
| 4 core | #193 | §7、§8(接続時・Topic・呼び出し時)(§19) |
| 5 Python | #194 | §11(§20) |
| 6 C++ | #195 | §11(§21) |
| 7 Haskell | #196 | §11(§22) |
| 8 CLI・管理画面 | #197 | §5.2(表示)、§6(§23) |
| 10 配布・`juiz_std_types` | #199 | §5.2(型パッケージの版)、§7.3(§24) |
| 13 ROS 2 の取り込み | #201 | §10 |
| Composite C8 | #172 | §7、§8 |
| 遅延評価 L1〜L3 | #175〜#177 | §12 |

## 14. 対象外(後の Step か、未定)

- logical type(時刻、decimal、uuid など)と一般の union。辞書での表し方を決めてから(§3.4)。
- `fixed`。ROS 2 の固定長配列は §10 のとおり属性で表す。
- 符号なし 64 bit 整数。
- 再帰する型。
- レコードの中の `image`。
- Avro のバイナリ形式での通信(QMP の高速化)。
- client stub(型付きで呼ぶ側の生成コード)。型と skeleton の後(計画書 §8 の 4)。Python は Step 5 で作った(§20.4)。

## 15. Rust の SDK(Step 1 #190 で決めたこと)

型のモデル・指紋・値の検査・互換性の判定はクレート `juiz_types`(juiz_sdk に依存しない。依存は serde_json・sha2・base64・regex・thiserror だけ)に置き、生成器 `juiz-idl`(Step 2)などはこれだけを使える。`juiz_sdk::types` は `juiz_types` を再公開し、`JuizType` と `TypeTag` を足す。`juiz_types` の関数は `juiz_types::TypeError`(`SchemaInvalid` / `Incompatible` / `ValueDoesNotMatch`)を返し、juiz_sdk の manifest・`JuizType` を通したときは §8 の名前の `JuizError` に変換する(`juiz_sdk::types::type_error`)。`type_name` は `juiz_types` では文字列(`argument_type_name`)、juiz_sdk では `ArgumentTypeOf::argument_type` が `ArgumentType` を返す。

| 名前 | 内容 |
|---|---|
| `DataType`(と `RecordType` / `RecordField` / `EnumType`) | §3 の型のモデル。`from_avro` / `to_avro`(自己完結、`doc`・`default`・`aliases` とその他の属性を残す)、`canonical_form`(PCF)、`fingerprint`、`display_name`、`argument_type_name`(§6 の表) |
| `TypeSchema` | §6 の型参照 `{name, fingerprint, avro}`。`validate` は `avro` を読み、`name` と `fingerprint` が計算したものと一致するかを見る。他の言語・ブローカーから届いた型参照は使う前にこれで検査する |
| `read_value` / `check_value` | §4 の規則で辞書を読む。`read_value` は reader から見た値(足りないフィールドを既定値で埋め、型にないキーを落とし、列挙の知らない記号を既定値にしたもの)を返す |
| `compare_types` / `ensure_compatible` / `Compatibility` | §7。`Compatibility` は `Untyped`(どちらかが型なし)・`Same`・`Compatible`・`Incompatible { path, reason }` |
| `JuizType` | 辞書との変換(`to_value` / `from_value`)、`data_type` / `type_schema` / `type_identifier`、`Capsule` / `CapsulePtr` との変換。`bool`・`i32`・`i64`・`f32`・`f64`・`String`・`Vec<u8>`(`bytes`)・`Vec<T>`・`HashMap<String, T>`・`Option<T>`・`Value`(`any`)・`DynamicImage`(`image`)・`()`(`null`)に実装済み |
| `TypeTag` | §12 の型タグ(完全名・指紋・ABI タグ)。`JuizType::abi_tag` が `None`(既定)なら `type_tag` も `None`。`destroy` / `clone` の関数は型ではなく言語ごとの登録所(Step 4)に置く |

- `#[derive(JuizType)]`(`juiz_macro`)は名前付きフィールドの struct を record に、フィールドのない変種だけの enum を enum にする。`#[juiz(namespace = "…")]` は必須、フィールドの既定値は `#[juiz(default = 値)]`(§4 の辞書の表し方、`serde_json::json!` に渡す)、列挙の既定値は変種に `#[juiz(default)]`。`///` は `doc` になる。ジェネリクスはエラー。
- `RecordField::default_value` は既定値を §4 の表し方で受け取り、Avro の表し方で持つ。`T?` は既定値が `null` なら `["null",T]`、それ以外なら `[T,"null"]` の並びにする(§9.2 と同じ)。
- 型の中の位置は、record のフィールドを `.名前`、配列の要素を値の検査では `[i]`・互換性の判定では `[]`、辞書の値を値の検査では `.キー`・互換性の判定では `{}` で表す。
- Avro のスキーマの中の名前の参照は、外側の名前空間を補った完全名、次に書かれたままの名前の順で探す。
- 基本型を `{"type": "long", …}` と書いたときの制約の属性(§10)は今は読み捨てる(Step 13 #201 で持つ)。
- 指紋のテストの期待値は apache-avro の `canonical_form()` で作った。apache-avro は serde_json の `arbitrary_precision` を有効にして Value の形を変える(Rust のプラグインと食い違う)ので、依存には入れない。

## 16. 生成器 juiz-idl(Step 2 #191 で決めたこと)

生成器はクレート `juiz_idl`(依存は `juiz_types` だけで、juiz_sdk に依存しない)。コマンド `juiz-idl` と `juiz idl` は同じ引数を取る。

```text
juiz-idl [-o 出力先] [-g avsc,json-schema,processes,rust] [--check] 入力.jidl|入力.avsc ...
```

- 入力は .jidl と .avsc(§9.4)。`import` した先も読み、**読んだすべてのファイルの名前付き型**を書き出す(生成した Rust のコードが参照する型をそろえるため)。`import` のパスは import を書いたファイルからの相対パス(見つからなければ import の探し先と組み込みの型、§24.1)。import の循環はエラー。
- 同じ完全名の型が複数のファイルにあるとき、中身(doc・既定値・aliases も含めた型のモデル)が同じなら 1 つにまとめ、違えばエラー。
- `-g` を省いたときは `avsc,json-schema,processes`。`--check` は書き出さずに検査し、型の識別子(§5.2)を表示する。中身が同じファイルは書き直さない。
- エラーは `ファイル:行:桁: 内容` の形。スキーマの規則(§3)の検査は `juiz_types` に任せる。

| 出力 | ファイル | 内容 |
|---|---|---|
| avsc | `<完全名>.avsc` | 名前付き型ごとの、それだけで完結したスキーマ(§6 の `avro` と同じもの) |
| json-schema | `<完全名>.schema.json` | §4 の辞書の表し方の JSON Schema 2020-12 |
| processes | `<namespace>.processes.json` | §9.3 のシグネチャ。同じ名前空間のファイルは 1 つにまとめる |
| rust | `<namespace>.rs`、`juiz_idl.rs` | struct / enum と `JuizType` の実装、名前空間のモジュールの木 |
| rust-skeleton | `<namespace>.skeleton.rs` | `process` / `container` 宣言の Rust の雛形(§18) |
| python | `<a>/<b>/__init__.py` | dataclass / enum.Enum と client stub(§20) |
| cpp | `<namespace>.h`、`juiz_idl.h` | struct / enum class と cppjuiz の `juiz::type_traits` の特殊化(§21) |

- JIDL の細部:
  - `/** … */` は宣言・フィールド・引数・`container_process` の前に書く。注釈との順番は問わない。行頭の `*` は落とす。
  - `union { null, T }` / `union { T, null }` は書いた並びを守り、既定値が先頭の型に合わなければエラー(Avro の決まり)。`T?` の並びは §9.2 のとおり既定値で決める。
  - `@namespace` で名前空間を変えた型は、ほかの型からは完全名で参照する。
  - 引数の既定値は §6 のとおり検査する(`image` と、`T?` でない型の `null` は検査しない)。`null` 単独・`void` は引数の型にできない。
- processes.json の引数は `name`・`type_name`(§6 の表)・`description`(あれば)・`schema`・`default`(あれば)を持つ。`void` の戻り値は `{"name": "null", …}`。
- JSON Schema:
  - record は `type: object`、既定値のないフィールドを `required` にする。型にないキーは読むときに無視する(§4)ので `additionalProperties` は付けない。
  - 参照する名前付き型は `$defs` に完全名で入れ、`{"$ref": "#/$defs/<完全名>"}` で参照する。
  - `int` / `long` は `integer` に範囲を付ける。`bytes` は `{"type": "string", "contentEncoding": "base64"}`、`T?` は `anyOf: [T, null]`、enum は `type: string` と `enum`、`any` は `{}`、`image` は `{"x-juiz-type": "image"}`。
  - 一番外に `x-juiz-fingerprint`(指紋)を付ける。互換性の判定には使わない(正本は .avsc)。
- Rust:
  - 名前空間 `a.b` の型は `a.b.rs` に置き、`juiz_idl.rs` が `pub mod a { pub mod b { include!("a.b.rs"); } }` の形で読み込む。使う側は `include!(concat!(env!("OUT_DIR"), "/juiz_idl.rs"));` と書く。ほかの名前空間の型は `super::…` の相対パスで参照するので、`juiz_idl.rs` を通して使う。名前空間のない型は Rust には書き出せない(エラー)。
  - record は `#[derive(Clone, Debug, PartialEq)]` の struct、enum は `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]` の enum(既定の記号があれば `Default` も)。型の対応は §11。
  - `JuizType::data_type` は .avsc と同じスキーマの文字列を埋め込んで読む(1 回だけ読んで使い回す)。したがって指紋・doc・aliases・既定値が .avsc と一致し、同じ形の `#[derive(JuizType)]` とも指紋が一致する。`to_value` / `from_value` は `#[derive(JuizType)]` と同じ。
  - serde の `Serialize` / `Deserialize` は付けない。serde の既定の表し方(`Vec<u8>` が数の配列になるなど)が §4 と違うため。辞書との変換は `JuizType` を使う。
  - フィールド名・記号が Rust のキーワードなら `r#type` のような raw identifier に、`self` / `Self` / `super` / `crate` なら末尾に `_` を付ける(辞書のキーは元の名前のまま)。型名・名前空間の要素にこの 4 つは使えない(エラー)。
  - `process` / `container` 宣言からの Rust のコード(skeleton)は §18。
- build.rs 用ヘルパ(tonic-build と同じ流れ)は別クレートにせず `juiz_idl::build` に置く: `juiz_idl::build::compile(&["idl/geometry.jidl"])` か `juiz_idl::build::configure().avsc(true).processes(true).compile(…)`。出力先の既定は `OUT_DIR`、読んだファイルの `cargo:rerun-if-changed` を出す。

## 17. Rust のマクロ(Step 3 #192 と遅延評価 L2 #176 で決めたこと)

`#[juiz_process]` 系の 6 マクロ(`juiz_process`・`juiz_container`・`juiz_container_process` と、それぞれの `juiz_component_*`)は、`JuizType` を実装した型を引数・戻り値に取れる。

```rust
#[juiz_process(arguments = { default = { target = { position = { x = 0.0, y = 0.0 } } } })]
fn move_to(target: Pose2D, speed: f64) -> JuizResult<Pose2D> { … }
```

- 引数:
  - 今までの型(`bool`・`i64`・`f64`・`String`・`Value`・`DynamicImage`・`Vec<Value>`・`Vec<f64>`・`Vec<i64>`)は今のとおり型なしの引数にする(`schema` なし、受け取り方も今のまま)。
  - それ以外の型は `JuizType` の型として、manifest の引数に `schema`(§6)を載せる(`add_typed_arg`)。`i32`・`f32`・`Vec<u8>`・`Vec<T>`・`HashMap<String, T>`・`Option<T>`・record・enum など。
  - 型付きの引数の既定値は、属性の `default` に §4 の辞書の表し方で書く(`pose = { position = { x = 0.0, y = 0.0 } }`)。書かなければ `null`(「既定値なし」、§6)。
  - 受け取りは `JuizType::extract(CapsulePtr)`。既定は `from_capsule_ptr`(辞書を読む)で、record はネイティブ値を受け取る(下)。
- 戻り値:
  - `JuizResult<Capsule>` / `JuizResult<JuizResponse>` は今のまま。`JuizResult<Value>` は型なしの値を返す(`output_schema` なし)。
  - それ以外の `JuizResult<T>` は、関数を `JuizResult<Capsule>` を返す形に書き換え、`JuizType::into_capsule` で包む。manifest には `output_schema`(と、`outputs` が空なら `type_name` だけの `outputs`)を載せる(`ProcessManifest::returns::<T>()`)。`JuizResult<()>` は `null` の型で、`Capsule::Empty` を返す。
- manifest を作るときのエラー(既定値が型に合わないなど)は、manifest の関数を呼んだとき(プラグインを読み込むとき)に関数の名前を付けて panic する。
- `juiz_container_process` の引数の並びは宣言順にした(今までは順不同だった)。
- 属性 `typed = "all"` を付けると、今までの型の引数と `JuizResult<Value>` の戻り値も型付きにする(`f64` は `double`、`Value` は `any` の `schema` を持ち、`JuizType::extract` で受け取る)。既定値を書かない引数は「既定値なし」になる。型付けは opt-in のままにするため、既定は今までどおり。
- 引数の既定値は `arguments = { default = "{\"speed\": 1.0}" }` のように JSON の文字列でも書ける(属性の書き方では負の数・配列・`null` を書けないため)。

### 17.1 ネイティブ値で渡す(遅延評価 L2)

- `#[derive(JuizType)]` と juiz-idl が生成する **record** は、`JuizType` の `abi_tag`・`into_capsule`・`from_capsule_ptr`・`extract` を実装し、戻り値を encode せずに `Capsule::Native(RustNative<T>)` で返す。そのため record の型には `Clone` が要る(生成コードは付けている)。enum・基本型・`Vec<T>`・`HashMap<String, T>`・`Option<T>` は今のとおり辞書で返す(小さい値は Native にしない、lazy-evaluation.md §4)。
- Rust の ABI タグ(§12)は `rust:<rustc の版>:<juiz_sdk の版>:<アドレス>`(`juiz_sdk::types::rust_abi_tag()`)。アドレスは juiz_sdk の中の static のもので、juiz_sdk はプラグイン(cdylib)ごとに静的にリンクされるので、**同じプラグインの中でだけ**一致する。
- 受け取り(`juiz_sdk::types::native_extract`):
  1. 届いた値が Native で、型タグ(完全名・指紋・ABI タグ)が自分の型のものと同じで、`RustNative<T>` にダウンキャストできる(同じ形の別の Rust の型でない)ときだけ、ネイティブ値のまま受け取る。ほかに参照が無ければ(Composite の中間値)値を**移し**、あれば(接続の memo を共有しているとき)`clone` する。encode は起きない。
  2. それ以外(別のプラグイン・型なし・他の言語の値)は、今のとおり辞書を読み(Native なら encode が起きる)、`from_value`(`read_value` で受け取る側の形にする)で作る。
- 戻り値を Rust 以外が読むとき(ブローカー越し・他の言語・profile)は、`to_value` で encode する(lazy-evaluation.md §5)。
- 足した API: `JuizType::extract`、`juiz_sdk::types::{RustNative, rust_abi_tag, native_into_capsule, native_from_capsule_ptr, native_extract}`、`NativeValue::into_any`(移すため。既定は `None`)、`NativeCell::try_into_value`、`Capsule::into_native_cell`、`CapsulePtr::try_into_capsule`、`ContainerManifest::add_typed_arg`。`NativeTypeTag` は `TypeTag` の別名にした(同じもの)。
- `process` / `container` 宣言からの Rust の skeleton の生成は §18。

## 18. Rust の雛形(skeleton、Step 3 #192 で決めたこと)

`juiz-idl -g rust-skeleton`(`juiz idl` も同じ)は、`process` / `container` 宣言のある名前空間ごとに `<namespace>.skeleton.rs` を書き出す。宣言のない名前空間は書き出さない。既定の `-g` には入れず、build.rs のヘルパ(`juiz_idl::build`)の既定にも入れない(雛形は一度作ってコピーし、手で本体を書くものなので)。`Builder::rust_skeleton(true)` は雛形を検査するテスト用。

- 中身は 1 つのコンポーネント(1 つの名前空間が 1 つの cdylib)。`process` は `#[juiz_component_process]`、`container` は状態の空の struct(名前は型名を CamelCase にしたもの。JIDL では Container の状態を型付けしない、§9.3)と引数のない `#[juiz_component_container]`、`container_process` は `#[juiz_component_container_process(container_type = …)]`、最後に `juiz_component_manifest!`。関数の本体は `todo!()`。
- マクロには `typed = "all"`(§17)を付け、説明は `description`、既定値は JSON の文字列で書く。したがってマクロが作る manifest の引数・戻り値の `type_name`・`schema`・`default` は、`<namespace>.processes.json` のシグネチャと一致する(`juiz_idl/test_package` で確かめている)。
- 型は `crate::a::b::T` の絶対パスで参照する。`juiz_idl.rs` をクレートの一番上で読み込む前提。型の対応は §11。
- 関数名・引数名はマクロがそのまま `type_name` と引数名にするので、Rust のキーワードの名前(`r#…` にしないと書けないもの)はエラーにする。

## 19. core の型検査と型の登録所(Step 4 #193 で決めたこと)

core の実装は `juiz_core::types` にある。

### 19.1 接続時

- Connection を作るとき(conf の `connections:`、`connection/create` の API、どちらも `connection_builder::connect` を通る)、source の profile の `output_schema`(W)と destination の profile の引数 `arg_name` の `schema`(R)を `juiz_core::types::check_connectable` で比べる。互換でなければ両端に何も足さずに `TypeIncompatibleError` を返す。
- `check_connectable` は、両方が型付きのとき、比べる前に両方の型参照を `TypeSchema::validate` で検査する(他の言語・ブローカーから届いた型参照なので、§15)。名前・指紋が合わなければ `TypeSchemaInvalidError`。
- リモートの Process の profile を取れないとき(相手が応答しないなど)は検査せず、今までの接続処理に任せる(警告をログに出す)。
- Composition C8(#172)の定義時の検査も `check_connectable`(子の戻り値を W、親の引数を R)を呼ぶ。

### 19.2 Topic

- Process が publish を始めるとき(`CoreWorker::process_publish_topic`)、その戻り値の型(W)を、Topic に今つながっている型付きの subscribe 側の引数の型(R)それぞれと比べる。subscribe を始めるとき(`process_subscribe_topic`)は逆に、その引数の型(R)を、今つながっている型付きの publish 側の戻り値の型(W)それぞれと比べる。互換でない組があればエラーにして、Topic にはつながない。
- Topic は型付きの相手の型を覚えておき、比べるときは Topic の Process に今つながっている相手の分だけを使う(消えた Process の型で断らない)。
- publish 側どうし・subscribe 側どうしは比べない。
- 同じ juiz プロセスの中の相手だけを比べる。サブシステムの Topic へ転送する先(`topic_push`)の型は比べない(後の課題)。
- あわせて、Process の出力側の接続が引数名をキーにしていたため、同じ名前の引数へ別々の Process をつなぐと前の接続が上書きされていた(Topic の subscribe 側が 1 つしか残らなかった)のを、接続の識別子をキーにするよう直した。

### 19.3 呼び出し時

- conf の `option.type_check.on_call` を `true` にすると、CoreBroker の `process_call`・`container_process_call`(ブローカー越しの `call` の入口)で、渡された引数の辞書を引数の型と §4 の規則で照らし合わせ、合わなければ関数を呼ばずに `ValueDoesNotMatchTypeError` を返す。既定は `false`(検査しない)。
  ```yaml
  option:
    type_check:
      on_call: true
  ```
- 同じ設定で `process_p_apply`・`container_process_p_apply` で拘束する値も検査する。
- `execute` には引数の辞書がないので検査しない(接続から届く値は接続時に、拘束値は p_apply のときに検査済み)。
- 型なしの引数、渡されていない引数(既定値を使う)、値でないもの(画像、Rust のネイティブ値)は見ない。
- 検査するのはこの juiz プロセスの Process だけで、リモートの Process は相手側の設定で検査する。

### 19.4 型の登録所と API

- 登録所(`juiz_core::types::TypeRegistry`)は、登録済みの Factory(Process・ContainerProcess・Composite)の manifest にある引数と戻り値の型参照から、名前付き型(record・enum)を入れ子も含めて集めたもの。問い合わせのたびに Factory から作るので、Factory を足し外しすれば中身も変わる(`CoreWorker::type_registry`)。予約名の `juiz.Any`・`juiz.Image` と名前のない型は載せない。サブシステムはたどらない。
- 同じ完全名で指紋の違う型は、別の型として両方載る。
- API(HTTP は `/api/type/…`、qmp・メッセンジャー系は汎用の経路。Rust は `TypeBrokerProxy`、`BrokerProxy` の親トレイト):

| API | 返すもの |
|---|---|
| `type/list` | `[{identifier, name, fingerprint, short_identifier, kind, used_by}]`(`identifier` の順)。`kind` は `record` / `enum`、`used_by` はその型を使っている Factory の型名(名前順) |
| `type/schema?identifier=…` | 型参照 `{name, fingerprint, avro}`(§6 と同じ形、`avro` は自己完結)。`identifier` は `<完全名>@sha256:…` か完全名だけ。完全名だけで指紋の違う型が複数あるか、見つからなければ `TypeNotFoundError` |

- 遅延評価の codec(L3 #177)は、この登録所には足さず、値と一緒に運ぶことにした(§21.3)。CLI の `juiz type list / show` と管理画面での表示は Step 8(#197、§23)。

## 20. Python(Step 5 #194 で決めたこと)

### 20.1 生成器の Python 出力

`juiz-idl -g python`(`juiz idl` も同じ)は、名前空間 `a.b` ごとにパッケージ `a/b/__init__.py` を書き出す。途中のパッケージ(`a/__init__.py`)も、型がなくても作る。出力先のディレクトリを `sys.path`(juiz の conf の `option.pythonpath`、`PYTHONPATH` など)に入れて `from robot.geometry import Pose2D` のように使う。既定の `-g` には入れない。

- record は `@dataclasses.dataclass(kw_only=True)`。フィールドの順番は宣言順で、既定値のないフィールドが既定値のあるフィールドの後ろに来てもよいようにキーワード専用にする(Python 3.10 以上)。既定値が `None`・真偽・数・文字列ならそのまま書き、それ以外(配列・辞書・record・enum・bytes・any)は `default_factory` で毎回作る。
- enum は `enum.Enum`。値は記号の文字列(§11)。
- どちらもメソッド `to_value()`(§4 の辞書にする)・`from_value(value, path="")`(§4 の規則で読む)・`type_schema()`(§6 の型参照 `{name, fingerprint, avro}`)と、クラス属性 `__juiz_type__`(型参照)を持つ。型参照は .avsc と同じスキーマを埋め込んだもので、指紋は .avsc・Rust の生成コード・`#[derive(JuizType)]` と一致する。
- 型の対応は §11 のとおり(`int`/`long` は `int`、`float`/`double` は `float`、`T?` は `T | None`、`any` は `typing.Any`、`image` は `PIL.Image.Image`)。型注釈は `from __future__ import annotations` の下で書く。
- ほかの名前空間の型は `import a.b as _ns_a_b` で参照する(名前空間どうしが互いに参照しても読み込める)。
- 名前の扱い: フィールド名・記号・引数名が Python のキーワードか、生成したメソッドの名前(`to_value`・`from_value`・`type_schema`、記号は `mro` も)なら末尾に `_` を付ける(辞書のキーは元の名前のまま)。型名・名前空間の要素がキーワードか `_` で始まるならエラー。名前空間のない型・宣言はエラー。
- 値の変換・検査は生成コードではなく pyjuiz の `juiz.datatype` が受け持つ(生成コードは pyjuiz に依存する。Rust の生成コードが juiz_sdk に依存するのと同じ)。

### 20.2 `juiz.datatype`

| 名前 | 内容 |
|---|---|
| `Null`・`Boolean`・`Int`・`Long`・`Float`・`Double`・`String`・`Bytes`・`Any`・`Image`、`Array(T)`・`Map(T)`・`Optional(T)`・`Record(cls)`・`Enum(cls)` | 型ごとの変換器。`to_value(x, path)` と `from_value(v, path)`。`from_value` は `juiz_types::read_value` と同じ規則(足りないフィールドは既定値、型にないキーは無視、列挙の知らない記号は既定値、`int` の範囲、`1.0` を整数として受け付けない、`bytes` は base64)。`to_value` は NaN・Infinity をエラーにする |
| `ValueDoesNotMatchTypeError` | 辞書が型に合わない(`type_name`・`path`・`reason`。Rust の同名のエラーと同じ文面) |
| `is_juiz_type(t)`・`type_schema(t)`・`converter_of(t)` | 生成した型かどうか、型参照の写し、変換器 |
| `coerce(cls, v, path)` | `cls` の値にする。`cls` の値ならそのまま、ほかの生成した型の値は辞書にしてから、辞書は `from_value` で読む |
| `to_value(x)` | 生成した型の値を辞書にする(リスト・辞書の中も)。それ以外はそのまま。`juiz.proxy` が引数を送るときに使う |
| `typed_call(proxy, arguments, returns)`、`OMIT` | client stub が使う(§20.4) |

### 20.3 pyjuiz のデコレータ

- `@juiz_process`・`@juiz_container`・`@juiz_container_process`(と component 版)は、引数・戻り値の型注釈が生成した型なら型付きにする。それ以外の型注釈(`int`、`dict`、生成していない dataclass など)は今のとおり型なし。`list[Pose2D]` のような生成した型を含む注釈はまだ使えない(今と同じく未対応の型のエラー)。
- 引数: manifest の引数に `schema` を載せ、`type_name` は §6 の表(record は `Object`、enum は `String`)。既定値は Python の既定値を `to_value` した辞書の表し方。既定値がなければ `default` を出さない(「既定値なし」、§6)。
- 呼び出し時、型付きの引数は `coerce` で生成した型の値にしてから関数に渡す。Python の Process が返した同じ型の値(遅延評価 L1 のネイティブ値)はそのまま渡り、辞書(ほかの言語・ブローカー・CLI から)は `from_value` で読む。読めなければ `ValueDoesNotMatchTypeError`(Process の呼び出しのエラーになる)。
- 戻り値: manifest に `output_schema` と、古いクライアント向けの `outputs`(`type_name` だけ)を載せる。関数が返した値は `coerce` で生成した型の値にし(辞書を返してもよい)、**辞書にしないで**返す。Python の値は遅延評価 L1 のとおり `Native(python)` に包まれ、Python 同士では変換せずに渡る。Python 以外が読むとき(Rust の Process、ブローカーの応答、profile)は、埋め込み Python 側(`python_plugin.rs` の `pyany_to_value`)が `__juiz_type__` を持つクラスの値を `to_value()` で辞書にする。
- 型注釈が文字列(`from __future__ import annotations`)でも、`inspect.signature(eval_str=True)` で型にして判定する。
- 生成していない dataclass の戻り値は今のとおり `dataclasses.asdict` で辞書にする。

### 20.4 型付きの client stub

- `process` 宣言 `move_to` からは `MoveToClient`、`container` 宣言 `motor` からは `MotorClient`(型名を CamelCase にして `Client` を付けたもの)を、型と同じモジュールに書き出す。名前が同じ名前空間の型とぶつかればエラー。
- `MoveToClient(proxy)` は `juiz.proxy.ProcessProxy` を、`MotorClient(proxy)` は `ContainerProxy` を包む。`connect(name, host="localhost", port=8000, namespace=None)` は HTTP で juiz につないで名前で探す。
- Process は `__call__`、Container は `container_process` ごとのメソッドで呼ぶ。引数は宣言の順の位置引数で、既定値のある引数より後ろの既定値のない引数はキーワード専用にする。既定値は `None`・真偽・数・文字列ならそのまま書き、それ以外は `juiz.datatype.OMIT`(渡さないで、呼ばれる側の既定値を使う)。引数名 `self` は `self_` にする。`container_process` の名前がキーワード・`proxy`・`type_name`・`connect`・`_` で始まる名前ならエラー。
- 呼ぶと、引数を `to_value` で辞書にして(生成した型の値でも辞書でもよい。型に合わなければ送る前に `ValueDoesNotMatchTypeError`)`call` し、戻り値を宣言の戻り値の型の `from_value` で読んで返す(`void` は `None`)。`image` の引数・戻り値は PIL の画像のまま。
- `juiz.proxy` の `ProcessProxy` / `ContainerProcessProxy` の `call`・`p_apply` も、生成した型の値を渡されたら `to_value` で辞書にして送る(戻り値は辞書のまま)。

### 20.5 テスト

- `bindings/pyjuiz/test/gen` は `juiz_idl/test_package/idl/geometry.jidl` から作った Python のコードで、リポジトリに入れておく。juiz_idl の `python_gen_is_up_to_date` が生成器の出力と一致するかを確かめる(作り直しは `JUIZ_IDL_UPDATE_PYTHON_GEN=1 cargo test -p juiz_idl --test idl_test python_gen_is_up_to_date`)。
- `bindings/pyjuiz/test/datatype_test.py`(変換・デコレータ・client stub、juiz を起動しない)、`juiz_core/tests/integration/python_datatype_test.rs`(manifest の型参照と Rust の指紋の一致、Python 同士でネイティブ値のまま渡ること、Python ↔ Rust の辞書の受け渡し、接続時の型検査、`bindings/pyjuiz/test/typed_client_test.py` で HTTP 越しの client stub)。

## 21. C++(Step 6 #195 と遅延評価 L3 #177 で決めたこと)

### 21.1 生成器の C++ 出力

`juiz-idl -g cpp`(`juiz idl` も同じ)は、名前空間 `a.b` ごとにヘッダ `a.b.h` と、全部を読み込む `juiz_idl.h` を書き出す。ヘッダだけで使え、cppjuiz の `juiz/types.h` に依存する(生成した Rust のコードが juiz_sdk に、Python のコードが pyjuiz に依存するのと同じ)。既定の `-g` には入れない。

- record は `struct`(フィールドは宣言順、既定値はメンバの初期化子。`==` / `!=` を持つ)、enum は `enum class`。型の対応は §11(`int` は `int32_t`、`long` は `int64_t`、`bytes` は `std::vector<uint8_t>`、`T?` は `std::optional<T>`、`map<T>` は `std::map<std::string, T>`、`any` は `juiz::Value`)。名前空間 `a.b` は C++ の `a::b`。
- 既定値は、基本型・`string`・enum・それらの `T?` はリテラル、それ以外(配列・辞書・record・bytes・any)は辞書のリテラルから `type_traits<T>::from_value` で作る。文字列のリテラルは `"`・`\`・ASCII 以外のバイトをエスケープする(ソースの文字コードに依らないように)。
- 型ごとに `juiz::type_traits<T>` を特殊化する。`type_info()`(型参照。.avsc と同じスキーマを埋め込み、指紋は libjuiz_sdk(Rust の juiz_types)が計算するので .avsc・Rust・Python と一致する)、`to_value` / `from_value`(§4 の辞書との変換)、`native`(record は true、enum は false)、`layout()`(ABI タグに入れる生成器の版 `juiz-idl-<版>`)。
- 参照する型が先に来るように並べ直す(record は値で持つので)。ほかの名前空間の型を参照するヘッダはそのヘッダを `#include` する。名前空間どうしが互いに参照するときはヘッダが書けないのでエラー(Rust・Python は書ける)。
- フィールド名・記号が C++ のキーワード(と `NULL`)なら末尾に `_` を付ける(辞書のキーは元の名前のまま)。型名・名前空間の要素がキーワードならエラー。名前空間のない型はエラー。
- `process` / `container` 宣言からは何も作らない(雛形・client stub は後、§14)。`image` は引数・戻り値の型そのものとしてだけ使える(§3.3)ので、生成器は何も作らない。C++ では `juiz::Image` で受け取り、返す(large-data.md §10、#253)。

CMake の関数 `juiz_generate_types()`(`bindings/cppjuiz/cmake/juiz_idl.cmake`)は、ビルドのときに `juiz-idl -g cpp` を実行してヘッダを作り、そのディレクトリをターゲットの include ディレクトリに足す。

```cmake
include(<juiz>/bindings/cppjuiz/cmake/juiz_idl.cmake)
juiz_generate_types(my_component IDL idl/geometry.jidl DEPENDS idl/common.jidl)
```

- `juiz-idl` は `JUIZ_IDL_EXECUTABLE` か PATH(と `JUIZ_IDL_HINTS`)から探す。import した先は `juiz-idl` の引数には要らないが、変わったときに作り直すには `DEPENDS` に書く。

### 21.2 cppjuiz(`juiz/types.h`)

- `juiz::type_traits<T>` が型と辞書(`juiz::Value`)の変換を受け持つ。基本型・`std::vector`・`std::map`・`std::optional`・`juiz::Value` には用意してある。型参照(`type_info()`)を持つ型を**型付きの型**(`juiz::is_typed_v<T>`)と呼び、生成した record・enum と `JUIZ_TYPE` で登録した型がこれに当たる。
- manifest: `ProcessManifest::add_typed_arg<T>(name, description[, 既定値の T])`(既定値を書かなければ「既定値なし」)、`add_typed_arg_value<T>(name, description, 辞書の既定値)`、`returns<T>()`(`output_schema` と、`type_name` だけの `outputs`)。引数の `type_name` は §6 の表。
- 引数: `PROCESS_FACTORY` などの関数の引数に型付きの型を `T` か `const T&` で書ける(`bind_process` / `bind_container_process` の多重定義)。受け取り方は次の 2 つ。
  1. 届いた値が型タグ(完全名・指紋・ABI タグ)の同じ C++ のネイティブ値なら、**複製せずに借りて**渡す(`capsule_ptr_borrow_native`)。`const T&` で受ければ複製も起きない。
  2. それ以外は、libjuiz_sdk が辞書を型で読み(`capsule_ptr_read_typed_value`。§4 の規則で、足りないフィールドを既定値で埋め、型にないキーを落とす。Native なら encode する)、`from_value` で作る。合わなければ `juiz::ValueDoesNotMatchTypeError` を投げ、マクロが `JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR`(-702)を返す。
- 戻り値: `std::optional<T>` の `T` に型付きの型を書ける(`serialize` の多重定義)。record(`native` が true)は**辞書にせず** C++ の値のまま `Capsule::Native` に入れ(`capsule_set_native`)、enum は辞書(記号の文字列)にする。`juiz::Value` を返す `serialize` も足した。
- 手で書いた型は `JUIZ_TYPE(T, Avro のスキーマの JSON, layout の文字列, to_value 関数, from_value 関数)` で登録する(IDL を使わないとき)。ネイティブ値で渡す。`layout` は T の定義を変えたら変える(ABI タグに入る)。
- 型ごとの static(`TypeInfo` と codec の表)は名前のない名前空間の関数に置き、プラグインごとに持つ。GCC はテンプレートや inline 関数の static を `STB_GNU_UNIQUE` にして、別々に開いたプラグインどうしで 1 つに共有してしまう(ABI タグや型の版の違うプラグインの値を取り違える)ため。プラグインの中の inline 変数も同じく共有されるので、プラグインごとの状態は `static` にする。
- あわせて直したもの: `juiz::Value` を辞書に書くときに `null` を落としていた(`T?` の空が消える)のを、`null` を書くようにした。そのため C++ の `ContainerManifest` の `arguments`(引数なしで `null`)は空の配列で書く。

### 21.3 C ABI(libjuiz_sdk、`juiz_sdk.h` に足した関数)

既存の関数は変えず、足しただけ(lazy-evaluation.md §11)。

| 関数 | 内容 |
|---|---|
| `juiz_type_new(avro_json, &t)` / `juiz_type_free(t)` | Avro のスキーマから型を作る。§3 に合わなければ `JUIZ_TYPE_SCHEMA_INVALID_ERROR`(-701、理由はログ) |
| `juiz_type_describe(t, callback, arg)` | `{"schema": 型参照, "type_name": manifest の type_name}` を `value` で渡す |
| `capsule_ptr_read_typed_value(cp, t, path, callback, arg)` | 引数の値を §4 の規則で型 `t` で読み、読んだ値を渡す。合わなければ `JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR`(-702、理由はログ) |
| `capsule_set_native(cp, obj, codec)` | C++ の値 `obj` を encode せずに戻り値に入れる。`obj` の持ち主は juiz になり、要らなくなったら `codec->destroy` で解放する |
| `capsule_ptr_borrow_native(cp, codec, &obj, &ref)` / `juiz_native_ref_release(ref)` | 型タグが `codec` と同じ C++ のネイティブ値なら貸す(`ref` を返すまで生きている)。違えば `JUIZ_NATIVE_NOT_MATCHED`(-703) |

- codec の表 `juiz_native_codec` は `{name, fingerprint, abi, encode, destroy}`。`encode(obj, capsule*)` は §4 の辞書を書く(Rust・Python・ブローカー越し・profile・ABI タグの違う C++ が読むときだけ呼ばれ、結果は覚えておく)。表はプラグインの中の static に置き、値と一緒に運ぶ(登録所には入れない。§19.4 の登録所は Factory の manifest から問い合わせのたびに作るので、そこに codec を置く場所がないため)。
- issue #177 の案にあった `decode` と `clone` は C ABI に入れなかった。`decode` は C++ の中で閉じる(`from_value`)。値は共有されたまま const で読むだけ(貸す)なので、juiz が C++ の値を複製することはない。

### 21.4 ABI タグ(§12)

C++ の ABI タグは `cpp:<コンパイラ>:<標準ライブラリ>:<layout>[:<JUIZ_CPP_ABI_TAG_EXTRA>]`(`juiz::cpp_abi_tag`)。

- コンパイラは `clang-<major>.<minor>` / `gcc-<major>.<minor>` / `msvc-<_MSC_VER>`、標準ライブラリは `libstdc++-<__GLIBCXX__>-cxx11abi<0|1>`(`_GLIBCXX_DEBUG` なら `-debug`)/ `libc++-<版>-abi<版>` / `msvcstl-<版>-idl<_ITERATOR_DEBUG_LEVEL>`。
- `layout` は型のメモリ上の形の版。生成した型は `juiz-idl-<版>`、`JUIZ_TYPE` の型は書いた文字列。
- 同じコンパイラ・同じ標準ライブラリ・同じ生成器の版でビルドしたプラグインどうしなら、**別のプラグインでも**ネイティブ値のまま渡る(Rust と違い、libjuiz_sdk は共有ライブラリで 1 つだけ読み込まれるので、値と codec を別のプラグインから読める)。ビルドの設定を区別したいときは `JUIZ_CPP_ABI_TAG_EXTRA` を定義する。

### 21.5 テスト

- `examples/cpp/component/cpp_typed_component`(`juiz_generate_types()` で `juiz_idl/test_package/idl/geometry.jidl` から型を作る。`JUIZ_TYPE` の `counted.Odometry` は encode・decode の回数を数える)と、同じ型を `JUIZ_CPP_ABI_TAG_EXTRA` で ABI タグを変えて受け取る `cpp_typed_component_other_abi`。
- `juiz_core/tests/integration/cpp_typed_test.rs`: manifest の型参照と Rust の指紋の一致、C++ どうしの Composite・接続で encode も decode も起きないこと、ABI タグの違う C++・C++ → Rust・Rust → C++ が辞書を通ること、辞書の引数が型で読まれること、すべての型のフィールドの往復、ContainerProcess の型付きの引数。
- `juiz_idl/tests/idl_test.rs` の `cpp_code`(生成したコードの形、名前空間の循環とキーワードのエラー)。

## 22. Haskell(Step 7 #196 で決めたこと)

### 22.1 生成器の Haskell 出力

`juiz-idl -g haskell`(`juiz idl` も同じ)は、名前空間 `a.b` ごとにモジュール `A.B`(ファイル `A/B.hs`)を書き出す。Template Haskell は使わない、ふつうの Haskell のソースで、hsjuiz に依存する(生成した Rust のコードが juiz_sdk に、Python が pyjuiz に、C++ が cppjuiz に依存するのと同じ)。既定の `-g` には入れない。cabal のパッケージの `hs-source-dirs` に出力先を足し、`other-modules` にモジュール名を書いて使う。

- record は record 構文の `data`(構築子は型と同じ名前、`deriving (Eq, Show)`)、enum は直和型(`deriving (Eq, Ord, Show, Enum, Bounded)`)。型の対応は §11(`int` は `Int32`、`long` は `Int64`、`float` は `Float`、`bytes` は `ByteString`、`T?` は `Maybe T`、`map<T>` は `Map Text T`、`any` は hsjuiz の `Value`)。
- 名前: モジュール名は名前空間の要素の先頭を大文字にしたもの、型名も先頭を大文字にする(英字で始まらなければエラー)。フィールドの関数は「先頭を小文字にした型名 + フィールド名を camelCase にしたもの」(`Pose2D` の `frame_id` は `pose2DFrameId`。Haskell の record のフィールド名はモジュールの中で重なれないため。辞書のキーは元の名前のまま)。列挙の構築子は「型名 + 先頭を大文字にした記号」(`MotorMode` の `Idle` は `MotorModeIdle`)。変換した名前がモジュールの中で重なる、または生成したコードが使う名前(`Maybe`・`Just`・`Value` など)と同じならエラー。名前空間の違う 2 つが同じモジュール名になってもエラー。名前空間のない型はエラー。
- 型ごとに、型の情報 `typeInfo<型名> :: TypeInfo`(.avsc と同じスキーマを埋め込み、指紋は libjuiz_sdk が計算するので .avsc・Rust・Python・C++ と一致する。`NOINLINE` のトップレベルの値で、最初に使ったときに 1 度だけ作る)と、`JuizType`・`FromCapsule`・`ToCapsule` のインスタンスを書く。`toValue` はすべてのフィールドを出し、`fromValue` は §4 の規則で読む(足りないフィールドは既定値、型にないキーは無視、列挙の知らない記号は既定値)。
- すべてのフィールドに既定値がある record には、その値 `default<型名>` も作る。
- ほかの名前空間の型は `import qualified A.B` で参照する。名前空間どうしが互いに参照するときはモジュールが書けない(hs-boot なしでは互いに import できない)のでエラー。生成したコードは Prelude を必要な名前だけ読み込み、ほかは修飾して使うので、JIDL の型名が `Text`・`Map` などでもぶつからない。
- `process` / `container` 宣言からは何も作らない(C++ と同じ)。`image` は引数・戻り値の型そのものとしてだけ使える(§3.3)ので、生成器は何も作らない。Haskell では `Juiz.Image` の `Image` で受け取り、返す(large-data.md §10、#253)。

### 22.2 hsjuiz

- `Juiz.Type`: 型付きの型のクラス `JuizType`(`juizType :: Proxy a -> TypeInfo`)、`TypeInfo`(`typeInfoName`・`typeInfoFingerprint`・`typeInfoSchema`)、Avro のスキーマから型を作る `newTypeInfo` / `unsafeTypeInfo`(libjuiz_sdk の `juiz_type_new` と `juiz_type_describe`。合わなければ `JUIZ_TYPE_SCHEMA_INVALID_ERROR`)、生成したコードが使う `recordFromValue`・`field`・`enumFromValue`。手で書いた型も、`unsafeTypeInfo` で作った型の情報とインスタンスを書けば型付きにできる(IDL を使わないとき。C++ の `JUIZ_TYPE` に当たる)。
- `FromCapsule` / `ToCapsule` に、既定の実装のあるメソッド `fromCapsuleType` / `toCapsuleType`(型付きの型なら `Just` の型の情報)を足した。今のインスタンスは変えなくてよい。あわせて `Float` と `ByteString`(base64)のインスタンスを足した。
- manifest: `addTypedArg 名前 説明 既定値`(既定値は型の値。型は既定値から決まる)、`addTypedArgValue (Proxy :: Proxy T) 名前 説明 辞書の既定値`、`addTypedArgNoDefault (Proxy :: Proxy T) 名前 説明`(既定値なし、§6)、`returns (Proxy :: Proxy T)`。引数の `type_name` は §6 の表。Container の引数(`containerManifest`)にも使える。
- manifest_entry_point(と Component の manifest)で、関数の型とマニフェストを比べる。関数の引数が型付きの型なら、マニフェストの引数も同じ型(名前と指紋が同じ)の型付きの引数でなければエラー(juiz は読み込むときに失敗する)。関数の引数が型なしの型(`Value` など)なら、型付きの引数も受け取れる。関数の結果が型付きの型なら、`returns` を書かなくても `output_schema` と、古いクライアント向けの `type_name` だけの `outputs` を載せる(`returns` を書いたときは同じ型でなければエラー)。`[Pose2D]` や `Maybe Pose2D` のような型付きの型を含む型は型なし(`fromCapsuleType` が `Nothing`)。
- 呼び出し時、型付きの引数は libjuiz_sdk が型で読み(`capsule_ptr_read_typed_value`。§4 の規則。ネイティブ値は encode する)、読んだ辞書を `fromValue` で Haskell の値にする。合わなければ `JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR`(-702、理由は juiz のログ)。
- 戻り値は今のとおり辞書にして返す。Haskell の値をネイティブ値(遅延評価 L1〜L3)で渡すことはしない(Haskell の値を C ABI 越しに貸すには StablePtr と型タグが要り、効果が出るのは Haskell どうしの接続だけなので、必要になってから)。

### 22.3 テスト

- `examples/haskell/src/TypedComponent.hs`(typed_component_hs。`examples/haskell/build.sh` が `juiz-idl -g haskell` で `juiz_idl/test_package/idl/geometry.jidl` から `examples/haskell/gen/` に型を作る。手で書いた型 `handwritten.Odometry` は `TypedOdometry.hs`)と、関数とマニフェストの型が合わない `BrokenTypedProcess.hs`(broken_typed_process_hs)。
- `juiz_core/tests/integration/haskell_typed_test.rs`: manifest の型参照と Rust の指紋の一致、`returns` を書かないときの `output_schema`、辞書の引数が型で読まれること、すべての型のフィールドの往復(C++ と同じ結果)、Haskell どうし・Haskell ↔ Rust の受け渡し、Container と ContainerProcess の型付きの引数、型の合わない Process を読み込めないこと。
- `juiz_idl/tests/idl_test.rs` の `haskell_code`(生成したコードの形、名前の変換と重なりのエラー、名前空間の循環のエラー)。

## 23. CLI・管理画面・OpenAPI・TypeScript(Step 8 #197 で決めたこと)

### 23.1 CLI `juiz type`

`-s` で指定した juiz(既定 `http://127.0.0.1:8000`)の型の登録所(§19.4)を見る。`juiz process` などと同じく、手元の設定ファイル(`-f`)で juiz を起動してから問い合わせる。

```text
juiz type list                                     # 識別子の JSON 配列(他の list と同じ形)
juiz type list --long                              # NAME KIND SHORT_ID USED_BY の表
juiz type show robot.geometry.Pose2D               # JIDL の宣言で表示
juiz type show robot.geometry.Pose2D --avro        # .avsc(それだけで完結した Avro のスキーマ)
juiz type show robot.geometry.Pose2D --json-schema # 辞書の表し方の JSON Schema(§16 の -g json-schema と同じ)
```

- `show` の識別子は `type/schema` と同じく `<完全名>@sha256:…` か、同じ完全名の型が 1 つなら完全名だけ。見つからない・決まらなければエラー(終了コード 1)。
- JIDL の表示は、先頭に識別子と使っている Factory の型名をコメントで書き、その型と入れ子の名前付き型の宣言を並べる(`juiz_idl::to_jidl`)。名前空間は調べた型のもので、ほかの名前空間の型には `@namespace("…")` を付ける。キーワードと同じ名前はバッククォートで囲み、`T?` で書けない並びの union は `union { T, null }` と書く。書いたものは juiz-idl で読み直せて、同じ型(指紋・既定値・doc・別名も同じ)になる。Avro の属性(`juiz.constants` など)は書かない。

### 23.2 管理画面

- 性質欄の引数の表の「型」は、型付きの引数なら型参照の名前(名前空間を省いたもの。全体の識別子は title)、型なしなら今どおり `type_name`。「出力」の戻値型も `output_schema` があればその名前。
- 処理・容器処理の性質欄に「呼び出す」を足した。引数ごとに入力欄を出し、`PATCH /api/process/call`(容器処理は `/api/container_process/call`)で呼んで結果を出す(画像の結果は画像で出す)。
  - 型付きの引数は profile の型参照の `avro` から入力欄を作る(`webui/src/avro.ts`)。record は入れ子の欄、enum は選択、`T?` は null のチェック、array・map は要素・キーの追加と削除、`any` は JSON の 1 行、`bytes` は base64 の文字列。
  - 型なしの引数は `type_name` から作る(`Int`・`Float` は数、`Bool` はチェック、`String` は文字列、`Array`・`Object` は JSON)。画像の引数はこの画面からは渡せない(省く)。
  - 初期値は manifest の既定値(辞書の表し方)。無ければフィールドの既定値か型ごとの空の値。「JSON で編集」で引数の辞書全体を JSON で書くこともできる。
  - 値は辞書の表し方(§4)のまま送る。型の検査は juiz 側の規則(§4、呼び出し時の検査 §19.3)に任せる。
- 画面は型の登録所の API を使わない(profile の型参照だけで足りる)。

### 23.3 OpenAPI(Swagger)

HTTP ブローカーの `/api-docs/openapi.json`(Swagger UI `/docs` が読むもの)は、問い合わせのたびに、静的な定義に次のものを足して返す(`juiz_core::types::openapi`)。Factory を足し外しすれば中身も変わる。

- `components.schemas` に、登録済みの Factory の manifest にある名前付き型(入れ子も含む)。キーは完全名。同じ完全名で指紋の違う型があれば `<完全名>_<指紋の先頭 8 桁>`(OpenAPI のキーに `@`・`:` は使えないため)。`x-juiz-fingerprint` に指紋を入れる。
- `components.schemas` に、Factory ごとの引数の辞書 `<種類>.<型名>.arguments`(`process.move_to.arguments`)と、戻り値が型付きなら `<種類>.<型名>.returns`。容器処理は `container_process.<Container の型>.<型名>`。型なしの引数は `type_name` から(`Int` は int64 の integer、`Image` は binary)。既定値は `default` に入れる。
- `PATCH /api/process/call`・`/api/container_process/call` の本体のスキーマを、Factory ごとの引数の辞書の `oneOf` にする(Swagger UI で選んで例を出せる)。どの Process を呼ぶかは `identifier` で決まるので、本体のスキーマで区別はしない。
- 書き方は utoipa が書き出す OpenAPI 3.0.3 に合わせる(JSON Schema 2020-12 の `-g json-schema` とは少し違う)。`T?` は `nullable: true`、`$ref` の隣に属性(`default`・`description`・`nullable`)を書くときは `allOf` で包む、`int`・`long`・`float`・`double` は `format`、`bytes` は `format: byte`、`required` が空なら書かない、`any` は `{}`。
- Process の `openapi_spec()`(`GET /api/process/openapi_spec?identifier=…`、容器処理は `/api/container_process/openapi_spec`)は、その Process だけの `{"arguments": 引数の辞書, "returns": 戻り値(型なしなら null), "components": {"schemas": 使う名前付き型}}` を返す(これまでは未実装で panic していた)。

### 23.4 生成器の TypeScript 出力

`juiz-idl -g typescript`(`juiz idl` も同じ)は、名前空間 `a.b` ごとに `a.b.ts` と、共通の `juiz_idl.ts` を書き出す。実行時のライブラリには依存しない(管理画面・`bindings/js` が JSON で受け渡す値の型)。既定の `-g` には入れない。

- record は `interface`(フィールドは辞書のキーのまま、すべて必須)、enum は文字列リテラルの union と、記号の配列 `<型名>Symbols`。すべてのフィールドに既定値がある record には `default<型名>()` も作る。型ごとに型参照 `<型名>Type: TypeSchema`(`{name, fingerprint, avro}`、指紋は .avsc と同じ)を書く。
- 型の対応: 数(`int`・`long`・`float`・`double`)は `number`(`long` は 2^53 を超えると精度が落ちる)、`bytes` は base64 の `string`、`T?` は `T | null`、`array<T>` は `T[]`、`map<T>` は `Record<string, T>`、`any` は `unknown`。
- ほかの名前空間の型は `import type * as a_b from "./a.b.js"` で参照する(型だけの import なので、名前空間どうしが互いに参照してもよい)。拡張子は `.js` と書く(TypeScript の `moduleResolution` が `bundler`・`nodenext` のどちらでも解決できる)。
- TypeScript の予約語・組み込みの型と同じ型名、ファイルの中で重なる名前、名前空間のない型はエラー。`process` / `container` 宣言からは何も作らない。

### 23.5 テスト

- `juiz_idl/tests/idl_test.rs` の `typescript_code`(生成したコードの形、名前の重なりのエラー)と `jidl_round_trip`(JIDL の表示を読み直すと同じ型)。生成した TypeScript は `tsc --strict`(`bundler`・`nodenext`)で通ることを手で確かめた。
- `juiz_app/src/types.rs` の単体テスト(表・JIDL・.avsc・JSON Schema の出力)と `juiz_app/tests/type_cli_test.rs`(native_pose_component を読み込んだ juiz を起動し、`juiz type list / show` を実行。ポート 8052)。
- `juiz_core/src/types/openapi.rs` の単体テストと、`juiz_core/tests/integration/type_check_test.rs` の `type_registry_api`(`/api-docs/openapi.json` と `process/openapi_spec`)。
- 管理画面は `npm run typecheck` と `npm run lint`。フォームから呼ぶところは手で確かめた。

## 24. 型パッケージの配布と標準の型 juiz_std(Step 10 #199 で決めたこと)

型パッケージは、JIDL(と手で書いた .avsc)と、そこから作った各言語のコードをまとめて配るもの。型の同一性は指紋で決まり、パッケージの版は識別子に入れない(§5.2)ので、**同じ JIDL から作ったものなら、どの言語・どのパッケージから使っても同じ型**になる。

### 24.1 import の探し先

`import idl "x/y.jidl";` / `import schema "x/y.avsc";` は次の順に探し、最初に見つかったものを読む。

1. import を書いたファイルからの相対パス(今まで通り)。組み込みのファイルの中では、組み込みのファイルどうしの相対パス。
2. import の探し先。`juiz-idl -I <dir>`(`--import-dir`、繰り返し可)、build.rs の `Builder::import_dir`、CMake の `juiz_generate_types(… IMPORT_DIRS <dir>)` で指定した順、次に環境変数 `JUIZ_IDL_PATH`(`PATH` と同じ区切りのディレクトリの並び)の順。
3. juiz-idl に組み込んだファイル(§24.4 の `juiz_std/…`)。

- どこにもなければ `ファイル:行:桁: imported file '…' is not found …` のエラー。
- コマンド・build.rs の入力に書いたパスは、ファイルがあればそれを、なければ組み込みのファイルを読む(`juiz-idl -g python juiz_std/geometry.jidl`)。
- 組み込みのファイルは build.rs の `cargo:rerun-if-changed` に出さない(juiz_idl の中にあるので、juiz_idl が変われば作り直される)。`JUIZ_IDL_PATH` には `cargo:rerun-if-env-changed` を出す。
- 同じ完全名の型が別のファイルにあるときの扱いは今まで通り(中身が同じなら 1 つにまとめ、違えばエラー、§16)。探し先の順で別のファイルが選ばれても、中身が同じなら結果は変わらない。

### 24.2 ほかのパッケージの型を参照する(extern)

`--extern <名前空間>[=<Rust のパス>]`(build.rs は `Builder::extern_path(名前空間, Rust のパス)`)を指定すると、その名前空間と下の名前空間(`juiz_std` なら `juiz_std.geometry` なども。`juiz_std2` は含まない)の型を**書き出さない**。型は import で読むので、指紋・検査・既定値の検査は今まで通り行う。

| 出力 | extern の型の扱い |
|---|---|
| avsc・json-schema・processes | その名前空間のファイルを書かない |
| rust・rust-skeleton | `<Rust のパス>::<残りの名前空間>::<型名>` を参照する(`juiz_std=::juiz_std_types::juiz_std` なら `::juiz_std_types::juiz_std::geometry::Pose2D`)。`juiz_idl.rs` にもその名前空間を入れない。Rust のパスがない extern の型を参照するとエラー |
| python | パッケージを書かない。参照は今まで通り `import juiz_std.geometry as …` なので、そのパッケージが入っていればよい(pyjuiz の `juiz_std`) |
| cpp | ヘッダを書かない。参照は今まで通り `#include "juiz_std.geometry.h"` なので、そのヘッダが include の探し先にあればよい |
| haskell | モジュールを書かない。参照は今まで通り `import qualified …` なので、そのモジュールを公開しているパッケージに依存すればよい |
| typescript | ファイルを書かない。参照は `./juiz_std.geometry.js` の相対パスのままなので、同じ出力先に置く |

- 使い分け: Rust と Python は extern を使い、型を持つパッケージのものを使う(同じ型が 2 つの Rust の型になるのを避ける)。C++・Haskell・TypeScript はふつう extern を使わず、import した型も一緒に作る(ヘッダ・モジュールはプラグインやアプリごとにビルドするので、作り直しても困らない)。

### 24.3 型パッケージの作り方と公開の手順

JIDL を正本にして、各言語のパッケージにはそこから作ったものを入れる。どの言語でも **.jidl をパッケージに入れて配る**(使う側がそれを import して自分の型のフィールドに使えるようにするため)。

- **Rust(crates.io)**
  - `idl/*.jidl` をクレートに入れ、build.rs で `juiz_idl::build::compile`、`src/lib.rs` で `include!(concat!(env!("OUT_DIR"), "/juiz_idl.rs"));`。依存は `juiz_sdk`、`[build-dependencies]` に `juiz_idl`(juiz の版に揃える)。
  - 使う側の build.rs が .jidl を見つけられるように、Cargo.toml に `links = "<クレート名>"` を書き、build.rs で `juiz_idl::build::export_idl_dir("idl")` を呼ぶ。直接依存するクレートの build.rs では、環境変数 `DEP_<links の値を大文字にし - を _ にしたもの>_IDL_DIR` にそのディレクトリの絶対パスが入るので、それを `import_dir` に渡し、`extern_path("<名前空間>", "::<クレート名>::<名前空間の先頭の要素>")` を足す(例は juiz_idl の README)。
  - 公開は `cargo publish`。初めて公開するクレートはトークンに publish-new の権限が要る。
- **Python(PyPI)**
  - `juiz-idl -g python -o <パッケージの置き場所> idl/*.jidl` で作ったパッケージ(名前空間 `a.b` → `a/b/__init__.py`)をコミットして wheel に入れる。依存は `pyjuiz`(生成したコードが `juiz.datatype` を使う)。
  - 自分の型が別の型パッケージの型を使うなら、`--extern <その名前空間>` を付けて作り、そのパッケージを依存に書く。
  - JIDL の名前空間の先頭がそのまま Python のトップレベルのパッケージ名になるので、PyPI のほかのパッケージとぶつからない名前空間にする。`juiz` は pyjuiz のパッケージ名なので使えない(型の名前空間としても予約、§3.4)。
  - 生成したコードが JIDL と一致するかを CI で確かめる(pyjuiz の `juiz_std` は juiz_idl の `pyjuiz_std_types_are_up_to_date` テスト)。
- **C++(CMake パッケージ)**
  - .jidl を `share/<パッケージ名>/idl` などに入れて配り、使う側は `juiz_generate_types(<target> IDL <使う .jidl> IMPORT_DIRS <その idl ディレクトリ>)` でヘッダを作る(ヘッダはビルドのたびに作るので、生成したヘッダは配らなくてよい)。CMake パッケージにするなら `<パッケージ名>Config.cmake` に idl ディレクトリの変数を置く。
- **Haskell(Hackage)**
  - `juiz-idl -g haskell -o src idl/*.jidl` で作ったモジュールをコミットして `exposed-modules` に書く(Template Haskell や Setup.hs で作らない。hsjuiz と同じく素の cabal で入るようにする)。依存は `hsjuiz`。.jidl は `extra-source-files` に入れる。
  - 別の型パッケージの型を使うなら `--extern <その名前空間>` を付けて作り、そのパッケージを `build-depends` に書く。
- **型を変えるとき**: 既存の型のフィールドの型・名前・順番を変えると指紋が変わり、古い版の型とは別の型になる(§7.3)。フィールドを足すときは既定値を付ける(互換のまま読める)。パッケージの版を上げても、指紋が同じなら同じ型として扱う。

### 24.4 標準の型 juiz_std

juiz-idl(`juiz_idl/std/juiz_std/*.jidl`)に組み込み、どこからでも `import idl "juiz_std/geometry.jidl";` で読める標準の型。名前空間は `juiz_std.*`(`juiz.*` は予約なので使わない)。長さは [m]、角度は [rad]、時間は [s]。**すべてのフィールドに既定値がある**(引数を省いても作れる)。

| ファイル | 名前空間 | 型 |
|---|---|---|
| `juiz_std/common.jidl` | `juiz_std.common` | `Time {sec, nanosec}`、`Duration {sec, nanosec}`、`Header {stamp, frame_id}` |
| `juiz_std/geometry.jidl` | `juiz_std.geometry` | `Point2D`、`Point3D`、`Vector2`、`Vector3`、`Quaternion`(既定は回転なし `w = 1`)、`Pose2D {position, theta}`、`Pose {position, orientation}`、`Twist2D {linear: Vector2, angular}`、`Twist {linear, angular}`、`Pose2DStamped`、`PoseStamped` |
| `juiz_std/sensor.jidl` | `juiz_std.sensor` | `ImageInfo {header, width, height, encoding}`(画像のメタ情報。画素は `image` 型で別に渡す、§3.3)、`Imu`、`LaserScan`、`Range` |

- 名前とフィールドは ROS 2 の `std_msgs`・`builtin_interfaces`・`geometry_msgs`・`sensor_msgs` に寄せた(Step 13 の取り込みで対応を付けやすくするため)。ただし juiz の型として別に定義したもので、`ros2.*` の型とは指紋が違う。
- 一度出した型は変えない(フィールドを足すときは既定値を付ける)。指紋は `juiz_idl/tests/idl_test.rs` の `std_type_fingerprints_are_fixed` で固定する。

| 言語 | 入手のしかた |
|---|---|
| Rust | クレート `juiz_std_types`(`juiz_std_types::geometry::Pose2D`)。build.rs で組み込みの JIDL から作る。自分の JIDL から参照するときは `extern_path("juiz_std", "::juiz_std_types::juiz_std")` |
| Python | pyjuiz に同梱のパッケージ `juiz_std`(`from juiz_std.geometry import Pose2D`)。自分の JIDL から作るときは `--extern juiz_std` |
| C++ | `juiz_generate_types(<target> IDL juiz_std/geometry.jidl)` か、自分の .jidl で import する(ヘッダは `juiz_std::geometry::Pose2D`) |
| Haskell | `juiz-idl -g haskell juiz_std/geometry.jidl`(モジュールは `Juiz_std.Geometry`、§22.1 の名前の付け方どおり) |
| TypeScript | `juiz-idl -g typescript juiz_std/geometry.jidl` |

### 24.5 テスト

- `juiz_idl/tests/idl_test.rs` の `builtin_std_types`(組み込みのファイルを入力・import に書ける、全フィールドに既定値)、`import_dirs`(`-I` の探し方と相対パスが先)、`extern_namespaces`(書き出さない・Rust のパス・Python の import)、`std_type_fingerprints_are_fixed`、`pyjuiz_std_types_are_up_to_date`。
- `juiz_std_types/tests/std_types_test.rs`(辞書との変換、既定値)と `juiz_std_types/test_package`(`extern_path` で juiz_std_types の型を参照する型のクレート)。
- `bindings/pyjuiz/test/std_types_test.py`(pyjuiz の `juiz_std` を Process の引数・戻り値に使う)。
- C++ は組み込みの JIDL と `IMPORT_DIRS` を使う `juiz_generate_types` でヘッダを作り、g++ でビルドできることを手で確かめた。Haskell の生成コードは §22 のテスト(`haskell_code`)と同じ生成器で作る。

