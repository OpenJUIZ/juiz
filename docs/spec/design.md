# 設計文書(juizdesign.yaml)仕様

- 状態: 確定(Step 0、issue #593、2026-11-23)。Step 1(#594、2026-11-23)で文書の読み書き・検査・プリセット・`juiz pkg design`・`design check` を実装し、実装に合わせて §3.1・§5・§9・§13 を書き足した
- 親 issue: #592。計画と経緯はプロジェクトの計画書 pkg-design-doc-plan.md(2026-11-23 確定、§6 の 9 項目はすべて推奨どおり)にある
- 関連: 雛形生成 [pkg.md](pkg.md)(§2.5〜§2.7 に CLI の 3 モード)、データ型 [datatype.md](datatype.md)(型の書き方 §9.2、辞書での値 §4、manifest §6、skeleton §18・§20.6・§21.6・§22.4・§26.4)、[manifest-profile.md](manifest-profile.md)
- この文書は Step 1〜6(#594〜#599)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す。「(Step n で決める)」と書いた所は、その Step の PR でこの文書に書き足す

## 1. 目的と用語

`juiz pkg` の雛形生成を二段階にする。Component の**設計文書** `juizdesign.yaml` を定め、雛形はこの文書から作る。

```
(-t のプリセット | .jidl | 手で書く) → juizdesign.yaml → 雛形(ソース・app.conf・README・ビルドのファイル)
```

- 設計文書は「どんな Process・Container・ContainerProcess を持つ Component を作るか」を、言語によらずに書いたもの。雛形のコードは言語ごとに違っても、設計文書は 1 つ。
- 今の `juiz pkg create`(`-t`・`--idl`)の動作は残し、雛形と一緒に設計文書も置く(pkg.md §2.2)。文書から雛形を作る `create -d`(pkg.md §2.5)と、文書だけを作る `juiz pkg design`(pkg.md §2.6)を足す。
- 作った雛形の型を、変えた文書に合わせて直すこと(sync)は扱わない(§11)。文書は「作り始めるときの設計」と「型の一覧の記録」で、`juiz pkg add` は文書にも型を足す(§8.3)。

| 用語 | 意味 |
| --- | --- |
| 設計文書、文書 | この仕様の YAML のファイル。既定の名前は `juizdesign.yaml` |
| 型 | Process・Container・ContainerProcess の Factory の段(`type_name` で呼ぶもの)。インスタンスではない |
| 型の式 | 引数・戻り値・状態の型を書く文字列。JIDL の型の書き方(datatype.md §9.2 の `type`) |
| プリセット | `-t component|process|container` に当たる、juiz_pkg に組み込んだ設計文書(§7) |
| 本体の断片 | プリセットの型の、言語ごとの関数の中身(`a + b` など、§7.2) |
| 未実装の印 | 本体の断片が無い型に入れる中身。今の `--idl` と同じ(Rust `todo!()`、Python `raise NotImplementedError`、C++ `throw std::runtime_error`、Haskell `error "…"`、Zig `return error.NotImplemented`) |

## 2. 方針

1. **1 つの文書 = 1 つの Component**。pkg.md §4 の「1 雛形 = 1 Component」と同じ。
2. **型(Factory)の段だけを書く**。インスタンス・接続・EC・ブローカーは今どおり `app/app.conf` に書く。文書から作る app.conf は、型ごとにインスタンスを 1 つ置いたもの(§6.3)。
3. **形式は YAML**(conf・`juizapp.yaml` と同じ)。雛形の直下に `juizdesign.yaml` の名前で置く(`app/juizapp.yaml` と並ぶ綴り)。
4. **型の書き方は JIDL を借りる**。record・enum の定義は .jidl に書き、文書の `idl:` で読む。文書の中には record・enum を定義しない(型を書く言語を 2 つにしない)。
5. **JIDL に無い 3 つを文書の側で足す**: Container を作るときの引数(`arguments`)、Container が持つ値(`state`)、ContainerProcess の `readonly`。
6. **言語によらない**。`component.language` は任意で、`create -d` の `-b` の既定値になるだけ。
7. 初版で書けるのは型(Process・Container・ContainerProcess)だけ。Topic の publish/subscribe、Composite、ContainerPose・ContainerModel、インスタンスは書けない(§11)。

## 3. 文書の形

### 3.1 例

`juiz pkg design my_robot`(`-t component` のプリセット、§7)が書く文書:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/OpenJUIZ/juiz/v<版>/juiz_pkg/schema/juizdesign.schema.json
# my_robot: design of the juiz component my_robot (juiz docs/spec/design.md)
#   check:    juiz pkg design check juizdesign.yaml
format: 1
component:
  name: my_robot
  description: "my_robot: a juiz component"
processes:
  - name: my_robot_add
    description: "Returns a + b."
    arguments:
      - { name: a, type: long, default: 0 }
      - { name: b, type: long, default: 0 }
    returns: long
containers:
  - name: my_robot
    description: "Holds an integer value."
    arguments:
      - { name: initial_value, type: long, default: 0 }
    state:
      - { name: value, type: long, init: initial_value }
    processes:
      - name: my_robot_increment
        description: "Adds step to the value and returns it."
        arguments:
          - { name: step, type: long, default: 1 }
        returns: long
      - name: my_robot_get
        description: "Returns the value."
        returns: long
        readonly: true
```

.jidl の record を使う例:

```yaml
format: 1
component:
  name: rover
  description: "Drives a two-wheeled rover."
  language: rust
idl:
  - idl/rover.jidl                 # namespace rover.types; record Velocity { double v; double w; }
  - juiz_std/geometry.jidl         # 組み込みの型(datatype.md §24.4)
processes:
  - name: rover_stop_command
    returns: rover.types.Velocity
containers:
  - name: rover_base
    arguments:
      - { name: wheel_base, type: double, default: 0.3, description: "distance between the wheels [m]" }
    state:
      - { name: wheel_base, type: double, init: wheel_base }
      - { name: last_command, type: "rover.types.Velocity?" }
      - { name: pose, type: juiz_std.geometry.Pose2D }
    processes:
      - name: rover_base_drive
        arguments:
          - { name: command, type: rover.types.Velocity }
        returns: juiz_std.geometry.Pose2D
      - name: rover_base_pose
        readonly: true
        returns: juiz_std.geometry.Pose2D
```

### 3.2 キーの一覧

最上位:

| キー | 必須 | 値 | 意味 |
| --- | --- | --- | --- |
| `format` | 必須 | 整数 `1` | 文書の形式の版。知らない値はエラー(「新しい juiz で作られた文書」と案内する) |
| `component` | 必須 | 辞書(下) | Component |
| `idl` | 任意 | 文字列の配列 | 読む .jidl(§4.3) |
| `processes` | 任意 | 配列(Process) | Component の Process の型 |
| `containers` | 任意 | 配列(Container) | Component の Container の型 |

`component`:

| キー | 必須 | 値 | 意味 |
| --- | --- | --- | --- |
| `name` | 必須 | 文字列 | Component の名前。pkg.md §3 の `<name>` の規則に従う(§5 の N1) |
| `description` | 任意 | 文字列 | Component の説明 |
| `language` | 任意 | `rust` / `python` / `cpp` / `haskell` / `zig` | `create -d` の `-b` の既定値(§8.2)。`c++` も `cpp` として受け、大文字小文字は区別しない(pkg.md §2.2 と同じ) |

Process(`processes[]`)と ContainerProcess(`containers[].processes[]`):

| キー | 必須 | 値 | 意味 |
| --- | --- | --- | --- |
| `name` | 必須 | 文字列 | 型名(`type_name`) |
| `description` | 任意 | 文字列 | 型の説明 |
| `arguments` | 任意 | 配列(引数) | 引数。並びは manifest の `arguments` の順番になる(datatype.md §9.3) |
| `returns` | 任意 | 型の式 | 戻り値の型。省くと `void` |
| `readonly` | 任意 | 真偽(既定 `false`) | **ContainerProcess だけ**。Container の状態を変えない ContainerProcess(transformations.md §14)。Process に書いたらエラー |

Container(`containers[]`):

| キー | 必須 | 値 | 意味 |
| --- | --- | --- | --- |
| `name` | 必須 | 文字列 | Container の型名 |
| `description` | 任意 | 文字列 | 型の説明 |
| `arguments` | 任意 | 配列(引数) | Container を作るときの引数。conf の `containers` の項目に書く値(§6.3) |
| `state` | 任意 | 配列(状態) | Container が持つ値(§3.3) |
| `processes` | 任意 | 配列(ContainerProcess) | この Container の ContainerProcess の型 |

引数(`arguments[]`):

| キー | 必須 | 値 | 意味 |
| --- | --- | --- | --- |
| `name` | 必須 | 文字列 | 引数名 |
| `type` | 必須 | 型の式 | 引数の型。型の無い引数は書けない(何でも入れたいときは `any`) |
| `default` | 任意 | 値 | 既定値。datatype.md §4 の辞書の表し方(§4.2) |
| `description` | 任意 | 文字列 | 引数の説明 |

状態(`containers[].state[]`、§3.3):

| キー | 必須 | 値 | 意味 |
| --- | --- | --- | --- |
| `name` | 必須 | 文字列 | 値の名前(状態の型のフィールド名) |
| `type` | 必須 | 型の式 | 値の型 |
| `init` | 任意 | 文字列 | 初期値にする Container の引数の名前 |
| `default` | 任意 | 値 | 初期値(§4.2)。`init` と一緒には書けない |
| `description` | 任意 | 文字列 | 値の説明(生成するコードのコメントになる) |

- 表に無いキーはエラーにする(綴りの間違いを見つけるため)。ただし `x-` で始まるキーはどこに書いてもよく、juiz は読まずに無視する(ほかの道具が情報を足すため。juiz_opm など)。
- 配列のキーに空の配列や `null` を書いたときは、省いたのと同じにする。
- 説明(`description`)の言葉は問わない。juiz が作る文書は英語で書く(pkg.md §4 と同じ)。

### 3.3 状態(`state`)

- `state` は、雛形の Container の状態の型(Rust の struct、Python のクラスの属性、C++ の struct、Haskell・Zig の `<Name>State` など)のフィールドになる。言語ごとの書き方は Step 2・Step 3 で決める。
- 初期値は次の順に決める。
  1. `init` があれば、その名前の Container の引数の値。引数は同じ Container の `arguments` に無ければならない。
  2. `default` があれば、その値。
  3. どちらも無ければ、型の 0 値(§4.4)。
- `init` で指す引数の型は、状態の型と同じでなければならない。ただし状態が `T?` で引数が `T` のときはよい。
- `state` は manifest にも conf にも出ない。雛形のコードを作るためだけの情報(§6.1)。
- `image` の状態は書けない(0 値が決まらず、Container の引数は conf の辞書なので `image` を渡せないため)。

## 4. 型と値

### 4.1 型の式

- `type` と `returns` は、JIDL の型の書き方(datatype.md §9.2 の `type`)の文字列で書く。`returns` だけは `void` も書ける。
  - 例: `boolean`、`int`、`long`、`float`、`double`、`string`、`bytes`、`any`、`image`、`array<double>`、`map<string>`、`long?`、`union { null, string }`、`rover.types.Velocity`、`array<juiz_std.geometry.Pose2D>`。
- **整数の型に注意**: JIDL の `int` は 32 bit、`long` は 64 bit(datatype.md §3.1)。今の `-t` の雛形の整数(Rust `i64`、C++ `int64_t`、Haskell `Int64`)は `long` に当たるので、プリセットは `long` で書く(§7)。
- record・enum は **完全名**(`<名前空間>.<名前>`)で書く。文書には名前空間が無いので、短い名前(`Velocity`)では解決しない。短い名前で書いて `idl:` の型に同じ名前のものがあれば、エラーに完全名の候補を出す。
- 型は `idl:` に書いた .jidl(と、それが import する .jidl・.avsc)の型から解決する。`idl:` に無いファイルの型は使えない(juiz_std の型も、`juiz_std/geometry.jidl` などを `idl:` に書いてから使う)。
- 使えない型: 引数の型の `null`(datatype.md §3.1)、`image?`・record のフィールドなどの中の `image`(datatype.md §3.3)、Container の引数と状態の `image`(§3.3)。`void` は `returns` だけ。
- 言語ごとの型の対応は datatype.md §11 のまま。
- YAML の flow 形式(`{ name: a, type: … }`)の中では、`?`・`{`・`}`・`,` を含む型の式を `"` で囲まなければならない(`type: "long?"`。囲まないと YAML として読めないか、別の値になる)。block 形式の `returns: long?` は囲まなくてよい。

### 4.2 既定値・初期値

- `default` は datatype.md §4 の辞書の表し方で書く(`bytes` は base64 の文字列、`T?` は `null` か T の値をそのまま、enum は記号の文字列、record はフィールド名 → 値の辞書)。
- 値は型で検査する(manifest を作るときの検査と同じ、datatype.md §6)。合わなければエラー。
- `default: null` で型が `T?` でないときは「既定値なし」として扱い、省いたのと同じにする(datatype.md §6 と同じ)。`image` の引数は既定値を持てない(書いたら無視して警告)。
- YAML の値は JSON の値に直して扱う。JSON にできない値(文字列でない辞書の鍵、`.nan`・`.inf`、`!!binary` などのタグ)はエラーにする。YAML の数の書き方(`0x10`・`1_000` など)は YAML 1.2 の core schema に従う(juiz の conf の読み方と同じ)。

### 4.3 `idl:`

- 値は .jidl のパスの配列。パスは**文書のあるディレクトリからの相対パス**で書く。`/` 区切り(Windows でも)。
- 相対パスで見つからなければ、組み込みのファイル(`juiz_std/…`、datatype.md §24.4)を探す。import の探し先(`-I`・`JUIZ_IDL_PATH`、datatype.md §24.1 の 2)は使わない(文書が置き場所によって違う型を読まないように)。
- `create -d` で雛形に写せるよう、組み込みでない .jidl と、それが import するファイルはすべて**文書のあるディレクトリの中**に無ければならない(外のファイルを指せばエラー。`..` で外へ出るパス、絶対パスもエラー)。雛形には文書からの相対パスのまま写す(§8.2)。置き場所は `idl/` を勧める(`juiz pkg design --idl` もそう書く)。
- `.avsc` は `idl:` に直接書けない(.jidl から `import schema` する)。
- `idl:` の .jidl にある `process` / `container` 宣言は読まない(型は文書が正本)。record・enum だけを使う。
- 読んだ .jidl の型の検査(名前空間の予約、再帰の禁止、既定値など)は juiz_idl と同じ。.jidl のエラーは `ファイル:行:桁:` の形でそのまま出す。

### 4.4 型の 0 値

状態の初期値(§3.3 の 3)と、conf に書く Container の引数の値(§6.3)と、生成後の表示の呼び出しの例(pkg.md §6.5)は、既定値が無いとき次の値を使う。

| 型 | 0 値 |
| --- | --- |
| `boolean` | `false` |
| `int`・`long` | `0` |
| `float`・`double` | `0.0` |
| `string` | `""` |
| `bytes` | `""`(空のバイト列) |
| `array<T>` | `[]` |
| `map<T>` | `{}` |
| `T?`・`any` | `null` |
| record | フィールドごとに、既定値があればそれ、無ければそのフィールドの型の 0 値 |
| enum | enum の既定の記号、無ければ最初の記号 |
| `image` | (無い。状態・Container の引数には書けない、呼び出しの例では `null`) |

## 5. 検査の規則

`juiz pkg design check`、`juiz pkg create -d`、`juiz pkg design`(書く前)は、同じ検査を通す。エラーがあればファイルを 1 つも書かない(pkg.md §8)。

エラーの場所は、文書の中の道筋(`containers[0].processes[1].arguments[0].type`)で示し、行と桁が分かれば `juizdesign.yaml:12:9:` の形で前に付ける。エラーは見つかった分をすべて並べる(1 つで止めない)。ただし YAML として読めないときと、`format` が違うときは、そこで止める。

実装(Step 1)での決めごと:

- エラーの位置は、そのキーの位置(`type: …` なら `type` の字の位置)。配列の要素は要素の位置。キーが無いエラー(「`name` が要る」など)は、そのキーを持つはずの辞書の位置。
- F1〜F3 のエラーがあれば、N・T の検査はしない(読めなかった値から別のエラーが続けて出ないように)。F3 のエラーはすべて並べる。
- N6・T6(言語の規則)は、ほかにエラーが無いときだけ見る。文書を juiz_idl のシグネチャに直し、型ごとに `*_skeleton_items` で作れるかを試して、作れなければその型(`processes[0]`・`containers[0].processes[1]` など)のエラーにする。Container の引数と状態の名前は、その名前の引数を 1 つ持つ関数を作れるかで確かめる。`idl:` の型は、その言語の型のコードを作れるか(juiz_idl の生成器)も確かめる。
- 文書には言語ごとの区別が無いので、N4 の `<Name>State` は言語によらず数える。

### 5.1 形

- F1. YAML として読める。最上位は辞書。
- F2. `format` が `1`。
- F3. §3.2 の表のキーと値の種類に合う。必須のキーがある。表に無いキー(`x-` で始まるものを除く)が無い。`readonly` は ContainerProcess だけ。
- F4. 型(Process・Container・ContainerProcess)が 1 つ以上ある。

### 5.2 名前

- N1. `component.name` は pkg.md §3 の規則(1〜4、言語が決まっていれば 5〜8 も)を満たす。
- N2. 型名(Process・Container・ContainerProcess)は、Avro の名前の規則(`[A-Za-z_][A-Za-z0-9_]*`)と `juiz_sdk::identifier::validate_name` を満たす。
- N3. 型名は文書の中で重ならない(Process・Container・ContainerProcess のすべてを通して)。`component.name` とは同じでよい(`-t container` のプリセットは Container の型名が `component.name`)。
- N4. 型名から作る識別子が重ならない。作る識別子は pkg.md §2.4 の一覧(型名と、それに `_manifest`・`_manif`・`_create`・`_entry_point`・`_manifest_entry_point` を付けたもの、Container の状態の型 `<Name>`(§3.1 の変換)、Haskell・Zig は `<Name>State`)。たとえば Container `arm` と Process `arm_create` は C++・Haskell・Zig で重なるのでエラー。
- N5. 引数名は Avro の名前の規則を満たし、1 つの型の中で重ならない。状態の名前も同じ規則で、1 つの Container の中で重ならない。
- N6. 言語が決まっているとき(§8.1)は、その言語の skeleton が作れない名前をエラーにする。キーワードの型名・引数名・状態の名前(datatype.md §18・§20.6・§21.6・§22.4・§26.4 の各言語の規則)と、pkg.md §6.5 の Haskell の「型のモジュール名が Component のモジュール名と同じ」もの。

### 5.3 型と値

- T1. `type`・`returns` が型の式として読め(§4.1)、record・enum の名前が `idl:` の型から解決できる。
- T2. §4.1 の使えない型を使っていない。
- T3. `default` が型に合う(§4.2)。record の値は、足りないフィールドに既定値があればよい(datatype.md §4 の読み方)。
- T4. 状態の `init` が同じ Container の引数を指し、型が §3.3 の規則で合う。`init` と `default` を両方書いていない。
- T5. `idl:` のパスが §4.3 の規則に合い、.jidl として読める。
- T6. 言語が決まっているとき、その言語の skeleton が作れない型を使っていない(Python・C++ の `T?`・`bytes` の引数、Python で既定値のある引数の後ろの既定値の無い record・enum の引数など、pkg.md §6.5)。

### 5.4 警告

エラーにはしないが表示するもの。終了コードは 0。

- W1. `create -d` の `-b` と `component.language` が違う(§8.2)。
- W2. `image` の引数の `default`(無視する、§4.2)。
- W3. `juiz pkg design check` で言語が決まらず、N6・T6 を確かめなかった(どの言語で作るかを `-b` か `language` で決めれば確かめられる、と案内する)。

## 6. ほかのものとの対応

### 6.1 manifest

文書から作った雛形のプラグインの manifest は、次のとおり文書の写しになる。Step 5(#598、任意)の `design check --plugin` は、この表で突き合わせる。

| 文書 | manifest(juiz_sdk) |
| --- | --- |
| `component.name` | `ComponentManifest.type_name` |
| `component.description` | `ComponentManifest.description` |
| (`-b` の言語) | `ComponentManifest.language` ほか各 manifest の `language` |
| `processes[]` | `ComponentManifest.processes[]`(`ProcessManifest`) |
| `containers[]` | `ComponentManifest.containers[]`(`ContainerManifest`) |
| `containers[].processes[]` | `ContainerManifest.processes[]`(`ProcessManifest`、`container_type` が Container の型名) |
| `name` | `type_name` |
| `description` | `description` |
| `arguments[]` | `arguments[]`(`ArgumentManifest`)。`name`・`description`・`default`・`schema`(datatype.md §6)。`type_name` は datatype.md §6 の表で型から決める |
| `returns` | `ProcessManifest.output_schema`(`void` なら無し)と、互換のための `outputs`(datatype.md §6) |
| `readonly` | `ProcessManifest.readonly` |
| `state` | (出ない) |
| `idl:` | (出ない。型は `schema` に展開されて入る) |

- 引数・戻り値は**すべて型付き**になる(Rust のマクロでは `typed = "all"`、datatype.md §17)。今の `-t` の雛形の引数は型の情報(`schema`)を持たないので、`-t` を文書経由にすると(Step 2)manifest に `schema` が付く。これは差分として許し、Step 2 の PR に書く(§7.3)。
- 既定値の無い引数の `default` は `null`(datatype.md §6 の「既定値なし」)。

### 6.2 JIDL の `process` / `container` 宣言

| 文書 | JIDL(datatype.md §9.3) |
| --- | --- |
| `component` | (無い。`--idl` では `<name>` が Component の名前、pkg.md §6.5) |
| `idl:` | 宣言のある .jidl とその import |
| `processes[]` | `process <returns> <name>(<arguments>);` |
| `containers[]` | `container <name> { … }` |
| `containers[].processes[]` | `container_process <returns> <name>(<arguments>);` |
| `description` | 宣言・引数の前の `/** … */` |
| `arguments[]` の `type`・`name`・`default` | `<type> <name> = <json_value>` |
| `returns`(省略) | `void` |
| `readonly`・Container の `arguments`・`state` | (無い) |

- .jidl から文書を作るとき(Step 4、`juiz pkg design --idl`・`create --idl`)は、宣言のある名前空間が 1 つでなければならない(pkg.md §6.5 と同じ)。型の参照は完全名に直し、`idl:` にはその .jidl を書く(§8.4)。Container の `arguments`・`state` は空、`readonly` は書かない(`false`)。
- 文書から雛形を作るときは、文書を juiz_idl のシグネチャの表現に直し、今の `*_skeleton_items` を「Container の引数・状態・readonly・本体の断片も出せる」ように広げたものを使う(言語ごとの生成器を新しく書かない、Step 2・Step 3)。

### 6.3 app/app.conf

文書から作る `app/app.conf` は、pkg.md §5.1 の形で、型ごとにインスタンスを 1 つ置く。

- `processes` に Process ごとに `type_name: <型名>`、`name: <型名>0`。
- `containers` に Container ごとに `type_name`・`name`(`<型名>0`)、Container の引数を 1 つずつ(値は既定値、無ければ §4.4 の 0 値)、`processes` に ContainerProcess ごとに `type_name`・`name`(`<型名>0`)。
- `ecs` は書かない(どの ContainerProcess を周期で呼ぶかは文書に無いため)。ただしプリセット(§7)は、今の `-t component`・`-t container` と同じ `ecs` を書く。
- Process・Container の無いブロックは書かない(今の `-t process` と同じ)。

### 6.4 そのほかの生成物

- README.md の型の表(pkg.md の `types_table`)は文書から作る(型名・種類・引数・戻り値・説明)。
- `app/juizapp.yaml` の `description` は `component.description`、無ければ今と同じ `"<name>: a juiz component"`。
- 生成後の表示(pkg.md §9)の呼び出しの例は、文書の型ごとに出す。引数は既定値、無ければ §4.4 の 0 値(`image` は `null`)。

## 7. プリセット(`-t`)

### 7.1 プリセットの文書

`-t component|process|container` は、組み込みの設計文書(プリセット)+ 言語ごとの本体の断片で表す。プリセットの文書は §3.1 の例(`component`)と、その部分集合。

| `-t` | 文書に入る型 |
| --- | --- |
| `component` | Process `<name>_add`、Container `<name>`(`_increment`・`_get`) |
| `process` | Process `<name>_add` だけ |
| `container` | Container `<name>`(`_increment`・`_get`)だけ |

- 型・引数・既定値・戻り値・説明は pkg.md §4 の表のとおり。整数は `long`(§4.1)。`<name>_get` は `readonly: true`。
- プリセットの文書は `juiz_pkg/templates/design/<template>.juizdesign.yaml.tmpl` に置き、pkg.md §10.2 の置換(`name`・`Name`・版)を通して作る。`component.description` は `"<name>: a juiz component"`(今の `juizapp.yaml` と同じ文)。

### 7.2 本体の断片

- プリセットの型の関数の中身(`a + b`、`value += step` など)と、プリセットだけが持つもの(Rust の `#[cfg(test)]` のテスト、`ecs` など)は、言語ごとの断片として持つ。置き場所と形は Step 2 で決める。
- 生成器は、型ごとに断片があればそれを、無ければ未実装の印を入れる。したがって、プリセットと同じ文書を `create -d` に渡すと、型の形は同じで本体は未実装になる(`-t` とは本体だけが違う)。
- 断片を選ぶ鍵は「プリセットの種類と型の役割(`add`・`create`・`increment`・`get`)」で、型名ではない(`<name>` によって型名が変わるため)。`create -d` には断片を使わない。

### 7.3 今の生成物との違い

- `-t` を文書経由に切り替えるとき(Step 2・Step 3)は、切り替える前の生成物を写したスナップショットのテストを先に足し、切り替えた後の差分を PR に理由と一緒に書く。差分はなるべく出さない。
- 許す差分: 雛形の直下に `juizdesign.yaml` が増える(Step 1 から)、manifest の引数・戻り値に `schema` が付く(§6.1)、それに伴うマクロ・デコレータの書き方の変化。
- サイトのチュートリアル第 1〜3 章は `-t process` と `pkg add` の出力を載せているので、出力が変わったら Step 6(#599)で直す。

## 8. CLI との関係

CLI の引数・表示は pkg.md §2.2・§2.5〜§2.7 に書く。ここでは文書の扱いだけを書く。

### 8.1 言語が決まるとき

- `create`・`create -d`: 雛形の言語(`-b`、`create -d` では §8.2)。
- `design`: `-b` を付けたときだけ。付けたら `component.language` にも書く。
- `design check`: `-b`、無ければ `component.language`。どちらも無ければ言語によらない検査だけをして W3 を出す。

### 8.2 `create -d`(文書から雛形)

- 言語は `-b`、無ければ `component.language`。どちらも無ければエラー。`-b` と `language` が違えば W1 を出して `-b` を使う。
- 名前は `component.name`。作り先は `-o`、省けば `./<component.name>`。
- 雛形の直下に文書を `juizdesign.yaml` として写す。中身はそのまま(コメントも残す)。ただし `-b` と `language` が違うときは、`component` の下の `language:` の行の値を書き換える(無いときは足さない)。行が見つからない形(flow 形式など)なら書き換えずに警告する。
- `idl:` の .jidl(と import するもの)は、文書からの相対パスのまま雛形に写す(§4.3)。組み込みのファイルは写さない。言語ごとのビルドのファイル(Rust の build.rs、C++ の `juiz_generate_types`、Haskell の dev.sh、Zig の `addTypes`)は、写したパスを読む(今の `--idl` の `idl/<file>.jidl` の代わり)。
- 作り先の規則は pkg.md §8 のまま。ただし作り先が文書のあるディレクトリそのもの(`-o .` など)のときは、その文書と `idl:` のファイル(と import するもの)だけは在ってよく、書き直さない。ほかのファイルがあればエラー。失敗したときは自分が書いたファイルだけを消す。これで「`juiz pkg design` で書いた文書を直して、同じディレクトリに雛形を作る」使い方ができる。
- 本体はすべて未実装の印(§7.2)。テストのコードは置かない(今の `--idl` と同じ)。

### 8.3 `juiz pkg add`(Step 4)

- `juiz pkg add process|container <name>` は、ソースと `app/app.conf` に加えて、雛形の直下の `juizdesign.yaml` にも、足す型をプリセットと同じ形(pkg.md §2.4 の表)で書き足す。`processes:`・`containers:` の配列の末尾に、§9 の書き方で足す。配列が無ければ文書の末尾に作る。
- `juizdesign.yaml` が無い(Step 1 より前に作った雛形)ときは、文書を直さずに警告する。
- 文書が §5 の検査を通らないとき、書き足す所が見つからない(flow 形式など)ときはエラーにし、手で足すよう案内する。3 つのファイルの書き換えは、全部の中身を作ってから書き、途中で失敗したら元に戻す(pkg.md §2.4 と同じ)。
- 書き足した文書が §5 の検査を通ることを確かめる。

### 8.4 `juiz pkg design --idl`(Step 4)

- `idl:` には、渡した .jidl の、出力する文書のディレクトリからの相対パスを書く。.jidl か、それが import するファイルが文書のディレクトリの外にあればエラー(§4.3。`-o` で置き場所を変えるよう案内する)。

## 9. 文書の書き方(juiz が書くとき)

`juiz pkg design`・`juiz pkg create`(`-t`・`--idl`)・`juiz pkg add` が書く文書は、次の形にそろえる(人が読み書きしやすく、`pkg add` が書き足す所を決めやすいように)。

- 先頭に §3.1 の 3 行のコメント。1 行目は JSON Schema(§10)の場所で、`<版>` は juiz の版(pkg.md §7.1)。`create -d` を実装する Step 2 で、4 行目に `#   scaffold: juiz pkg create -d juizdesign.yaml -b <rust|python|cpp|haskell|zig>` を足す(Step 1 の版では、まだ無いコマンドを案内しないため)。
- キーの順番は §3.2 の表の順。値の無い任意のキーは書かない。
- 型は block 形式(`- name: …` の後に字下げしたキー)、引数と状態は 1 行の flow 形式(`- { name: a, type: long, default: 0 }`)。`readonly: true` は ContainerProcess の最後のキー(`false` なら書かない)。
- 既定値・初期値は 1 行の flow 形式で書く。文字列はいつも `"` で囲み、辞書の鍵は名前の順(serde_json の並び)。
- 文字列は、`description` と、英数字・`_`・`.`・`<`・`>` 以外の字を含む型の式(`"long?"`・`"union { null, string }"`)を `"` で囲む。名前とそれ以外の型の式は囲まない(`array<double>` は YAML でそのまま文字列になる)。ただし、囲まないと文字列でない値に読めるもの(`null`・`true`・`10` など)は囲む。`idl:` のパスは英数字・`_`・`.`・`/`・`-` だけなら囲まない。
- 字下げは 2 つの空白、改行は LF、UTF-8(BOM なし)。

利用者が書く文書は、この形でなくてよい(YAML として正しければ読む)。

## 10. JSON Schema

- `juiz_pkg/schema/juizdesign.schema.json`(JSON Schema 2020-12)に置く。エディタ(VS Code の YAML 拡張など)で補完と形の検査に使う。文書の 1 行目のコメント(§9)で、その版のタグの raw URL を指す。
- JSON Schema で確かめるのは §5.1 の F1〜F3 と、名前の文字の形(N2・N5 の正規表現)まで。名前の重なり・型の解決・既定値の検査・言語の規則は `juiz pkg design check` で確かめる。
- `component.language` の値は、JSON Schema では小文字だけを通す(juiz は大文字も受ける、§3.2。juiz が書く文書は小文字)。
- juiz_pkg は文書を読むときに JSON Schema を使わない(エラーの場所と文を自分で出すため)。JSON Schema と juiz_pkg の検査が食い違わないことを、juiz_pkg のテストで確かめる(プリセットと §3.1 の例が JSON Schema を通り、§5.1 の F3 に反する例が JSON Schema でも落ちる)。

## 11. 後回しにするもの

| もの | 今の扱い | 後で足すなら |
| --- | --- | --- |
| 変えた文書で既存の雛形を直す(sync) | しない(計画 §6 の 7) | 利用者が書いた本体を壊さずに直す方法が決まってから |
| Topic の publish / subscribe | 書けない(F3 のエラー) | `processes[].publishes` など。manifest の `publishes`・`subscribes` に写す |
| Composite・インスタンス・接続・EC | 書けない。app.conf に書く | 別の文書(System の設計)として |
| ContainerPose・ContainerModel | 書けない | Container に `pose`・`model` |
| 戻り値の説明 | 書けない(`returns` は型の式だけ) | `returns: { type: …, description: … }` も受ける |
| 型の短い名前 | 完全名だけ | 文書に既定の名前空間を書くキー |
| 文書の中の record・enum | 書けない(.jidl に書く、計画 §6 の 4) | |
| 逆向き(プラグインの manifest と文書の突き合わせ、プラグインから文書を起こす) | Step 5(#598、任意)で `design check --plugin`・`design --from-plugin` | |
| 複数 Component の文書 | 書けない(1 文書 = 1 Component) | |

## 12. テスト

- Step 1: 文書の読み書きと §5 の検査の、合う例・合わない例(規則ごとに 1 つ以上、エラーの道筋と行を含めて)。プリセット 3 つと §3.1 の例が検査を通り、juiz が書いた文書を読んで書き直すと同じ文字列になる。JSON Schema との食い違い(§10)。`create -t` の雛形に `juizdesign.yaml` が置かれる(5 言語 × 3 種類)。`juiz pkg design`・`design check` の CLI。
- Step 2・3: 切り替え前のスナップショットとの比較(§7.3)。`create -d` で §3.1 の 2 つの例から雛形を作り、本体を書いてから生成から配布まで(pkg.md §11 と同じ形、pkg_cli_test)を通し、プラグインの manifest が §6.1 のとおり文書の写しになっていること。Container の引数・状態・readonly が言語ごとのコードに出ること。
- Step 4: `--idl` の今のテストが文書経由でも通ること、`pkg add` が文書に書き足し、書き足した文書が検査を通ること、`juizdesign.yaml` の無い雛形への `pkg add` が警告で通ること。

## 13. Step との対応

| Step | issue | この文書の節 |
| --- | --- | --- |
| 0 仕様書 | #593 | 全体 |
| 1 文書の読み書き・検査・プリセット、`create -t` で文書を置くだけ、`juiz pkg design`(`-t`)・`design check` | #594 | §3、§4、§5、§7.1、§8.1、§9、§10、§12 |
| 2 文書 → 雛形(Rust・Python)、`create -d`、`-t` を文書経由に | #595 | §3.3、§6、§7.2、§7.3、§8.2 |
| 3 文書 → 雛形(C++・Haskell・Zig) | #596 | 同上 |
| 4 `--idl` を文書経由に、`juiz pkg design --idl`、`pkg add` が文書も直す | #597 | §6.2、§8.3、§8.4 |
| 5(任意)逆向き | #598 | §6.1、§11 |
| 6 サイト・サンプル・リリース | #599 | §7.3 |
