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
| 呼び出し時 | ブローカー越しの `call` / `execute` の入口で、引数の辞書を §4 の規則で引数の型と照らし合わせ、合わなければ関数を呼ばずにエラー | **オフ**。conf で入れる(キーは Step 4 で決める) |

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
      "arguments": [ { "name": "target", "schema": { … §6 … } },
                     { "name": "speed", "schema": { … }, "default": 1.0 } ],
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
| `image` | `DynamicImage` | `PIL.Image.Image` | 今の cppjuiz の画像型 | 今の hsjuiz の画像型 | (扱わない) |

- 生成コードは型ごとに、スキーマ(§6 の `TypeSchema`)を返す関数と、§4 の辞書との変換(`to_value` / `from_value` に当たるもの)を持つ。
- Rust の `#[derive(JuizType)]` は、Rust の struct / enum から同じスキーマを作る。名前空間は `#[juiz(namespace = "robot.geometry")]` で書き、省略時はエラー。フィールドの順番は宣言順。JIDL で同じ形に書いた型と指紋が一致しなければならない。derive は Rust の中で閉じ、他の言語へは JIDL(または derive が書き出す .avsc)経由で渡す。
- pyjuiz で生成コードを使わない今の dataclass は、今のとおり型なし(`Object`)のまま扱う。

## 12. 遅延評価(#3)の型識別子

遅延評価(docs/spec/lazy-evaluation.md §9)は、ネイティブ値を渡してよいかの判定に次の**型タグ**を使う。

- 型タグ = 完全名 + 指紋(§5)+ **ABI タグ**。
- ABI タグは、同じメモリ表現であることを保証する範囲を表す文字列で、**完全一致だけ**で比べる(中身を解釈しない)。言語ごとの中身は次を基準にし、L1〜L3 で決める。
  - Rust: `rust:<rustc の版>:<cdylib を一意に表すもの>`(L0 §9 のとおり、最初は同じプラグインの中だけ)
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
| 2 生成器 `juiz-idl` | #191 | §5、§9、§11(Rust、JSON Schema) |
| 3 Rust マクロ | #192 | §6、§11(`#[juiz_process]` 系が `JuizType` を受ける) |
| 4 core | #193 | §7、§8(接続時・Topic・呼び出し時) |
| 5 Python | #194 | §11 |
| 6 C++ | #195 | §11 |
| 7 Haskell | #196 | §11 |
| 8 CLI・管理画面 | #197 | §5.2(表示)、§6 |
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
- JSON Schema への書き出しの細部(管理画面のフォーム・Swagger 用、Step 2 / 8 で決める)。
- client stub(型付きで呼ぶ側の生成コード)。型と skeleton の後(計画書 §8 の 4)。
- 型の登録所の API の形(`/type/list`、`/type/schema`、Step 4 で決める)。

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

