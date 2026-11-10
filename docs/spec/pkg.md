# 雛形生成コマンド(juiz pkg)仕様

- 状態: 確定(juiz pkg Step 0、issue #456、2026-11-09)。Step 1(#457、2026-11-10)で Rust・Python を実装し、実装に合わせて §2.2・§5・§6.1・§6.2・§10.2・§11 を書き足した。Step 2(#458、2026-11-10)で C++ を実装し、§2.2・§3・§5.2・§6.3・§7.1・§9・§10.2・§11 を書き足した。Step 4(#460、2026-11-10)で `--idl` を Rust だけ実装し、§2.2・§6.5・§7・§9・§10・§11・§12 を書き足した。Step 5(#461、2026-11-10)で `juiz pkg add` を実装し、§1・§2.4・§10.1・§11・§12 を書き足した
- 親 issue: #455。検討の経緯と比較はプロジェクトの計画書 juiz-pkg-plan.md(2026-11-09 確定)にある
- 関連: アプリ配布形式 [app-package.md](app-package.md)(雛形はそのまま `juiz pack` できる形にする)、Rust プラグインの C ABI [rust-plugin-abi.md](rust-plugin-abi.md)(雛形の Rust プラグインは C ABI)、データ型 [datatype.md](datatype.md) §18(`rust-skeleton`、Step 4 の `--idl`)
- この文書は Step 1〜6(#457〜#462)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す。「(Step n で決める)」と書いた所は、その Step の PR でこの文書に書き足す

## 1. 目的と範囲

Process・Container・Component を作り始めるときの「最初のプロジェクト」を、コマンド 1 つで作れるようにする。

```
juiz pkg create my_robot -b rust
```

- 作るものは「1 つの `.juizapp` になるもの」(app-package.md)。生成した直後から、ビルド → `juiz -f app/app.conf` で起動 → 呼び出し → `juiz pack` で配布、が手を加えずに通らなければならない。
- 中身の形は、チュートリアル(サイトの第 1〜5 章の言語別「1 から作る形」)と、実際のアプリのリポジトリ teleop_twist(`app/app.conf` + `app/juizapp.yaml` + `scripts/`)の形をそのまま使う。新しい形は作らない。
- 雛形に書く juiz の SDK の版は、雛形を作った juiz 自身の版にする(§7)。チュートリアルで一番つまずく「`juiz --version` と同じ版にそろえる」を機械的に満たすため。
- 対象の言語は Rust・Python・C++・Haskell。

扱わないもの(§12 で後回しにしたもの)は、外部のテンプレート(git から取るもの、cargo-generate 互換)、既存の雛形への型の追加(`juiz pkg add`、§2.4)以外の書き換え、Haskell の Windows。

### 1.1 用語

| 用語 | 意味 |
| --- | --- |
| 雛形 | `juiz pkg create` が作るディレクトリ一式。1 つの雛形は 1 つの Component と 1 つの `.juizapp` に対応する |
| 名前 | `create` に渡す `<name>`。Component の名前、パッケージ名、ビルドの成果物の名前、型名の頭になる(§3) |
| テンプレート | 雛形の元になるファイルの集まり。juiz のバイナリに埋め込む(§10) |
| テンプレートの種類 | `-t` で選ぶ `component` / `process` / `container`(§4) |
| ホストのターゲット | 雛形を作った・動かす juiz のターゲット(`juiz_core::HOST_TARGET`、app-package.md §5) |

## 2. コマンド

### 2.1 置き方

- 本命は juiz のサブコマンド `juiz pkg`。cargo の `juiz`(juiz_app)、pip の `juiz-cli`、Releases の juiz のどれにも入る。feature では切らない(テンプレートは数十 KB のテキストなので)。
- 新しいクレート `juiz_pkg`(lib + bin)を作る。lib に引数の定義(clap の `PkgArgs`)と生成の処理を置き、juiz_app はそれをサブコマンド `pkg` として取り込む。bin `juiz-pkg` は同じ `PkgArgs` をそのまま使う。`juiz idl` と `juiz-idl`(juiz_idl クレート)と同じ形。
- `juiz-pkg <サブコマンド> …` と `juiz pkg <サブコマンド> …` は同じ引数を受け、同じ動作をしなければならない。案内(サイト・README・生成後の表示)では `juiz pkg` を使う。
- `juiz pkg` は System を起動しない(`juiz idl`・`juiz pack` と同じく、ログの初期化と System の組み立ての前に処理して終わる)。
- 版の一致: `juiz-pkg` を `cargo install juiz_pkg` で別に入れたときは、juiz_pkg の版が埋まる(§7)。juiz と版がずれうるので、案内は `juiz pkg` にする。

### 2.2 `juiz pkg create`

```
juiz pkg create <name> -b <lang> [-t <template> | --idl <file.jidl>] [-o <dir>]
                [--license <mit-apache|none>] [--author <name>]
                [--hsjuiz-dir <dir>] [--juiz-sdk-dir <dir>] [--juiz-source <dir>]
```

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `<name>` | (必須) | 名前。§3 の規則に合わなければエラー |
| `-b`, `--lang <lang>` | (必須) | `rust` / `python` / `cpp` / `haskell`。`c++` も `cpp` として受ける。大文字小文字は区別しない |
| `-t`, `--template <template>` | `component` | `component` / `process` / `container`(§4) |
| `--idl <file.jidl>` | なし | §4 の例の型の代わりに、.jidl の `process` / `container` 宣言から型付きの関数を作る(§6.5)。`-b rust` だけ(Step 4)。`-t` とは一緒に使えない |
| `-o`, `--output <dir>` | `./<name>` | 雛形を作るディレクトリそのもの(親ではない)。§8 の規則で既にあれば止める |
| `--license <kind>` | `none` | `mit-apache` で LICENSE-MIT・LICENSE-APACHE を置き、各ファイルの license の欄を `MIT OR Apache-2.0` にする(§5.5)。`none` では置かない |
| `--author <name>` | `git config user.name` | `--license mit-apache` のときの著作権者。省いて git からも取れなければエラー |
| `--hsjuiz-dir <dir>` | `~/juiz-<版>/bindings/hsjuiz` | Haskell のみ。cabal.project に書く hsjuiz のソースの場所(§6.4) |
| `--juiz-sdk-dir <dir>` | `~/.juiz/sdk/<版>` | C++・Haskell のみ。libjuiz_sdk を展開した場所(§6.3・§6.4) |
| `--juiz-source <dir>` | なし | 開発者向け。juiz のリポジトリのソースを使う(§7.2)。juiz 自身のテストと CI で使う |

- 言語・テンプレートの値が違えば、受け付ける値を並べてエラーにする。
- 4 つの言語すべてにテンプレートがある(Step 3 で Haskell を足して、テンプレートの無い言語のエラーは無くした)。
- 言語に関係しないオプション(`-b rust` に `--hsjuiz-dir` など)は、警告を出して無視する。
- 成功したら終了コード 0、失敗したら 0 以外。失敗したときは何も残さない(§8)。
- 表示は英語(juiz の CLI のほかの表示に合わせる)。

### 2.3 `juiz pkg target`

```
juiz pkg target
```

ホストのターゲット(`juiz_core::HOST_TARGET`、例 `x86_64-unknown-linux-gnu`)を 1 行で表示する。雛形の `scripts/dev.sh` などが、ビルドしたプラグインを `app/plugins/<ターゲット>/` に置くときに使う(rustc の無い C++・Haskell の環境でも、conf の `{target}` が置き換わる名前と必ず一致させるため)。

### 2.4 `juiz pkg add`(Step 5)

```
juiz pkg add process <name> [-C <dir>]
juiz pkg add container <name> [-C <dir>]
```

`juiz pkg create` で作った雛形の Component に、型を足す。

| 引数 | 足す型 | 中身 |
| --- | --- | --- |
| `process <name>` | Process `<name>` | §4 の `<name>_add` と同じ(`a: int`(0)、`b: int`(0) → `a + b`)。型名は `<name>` そのもの |
| `container <name>` | Container `<name>`、ContainerProcess `<name>_increment`・`<name>_get` | §4 の `<name>`・`<name>_increment`・`<name>_get` と同じ(`_get` は readonly) |

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `-C`, `--dir <dir>` | `.` | 雛形のディレクトリ(`app/app.conf` があるところ) |

- 言語と Component の名前は `app/app.conf` の `plugins.components` から読む。Component が 1 つでない、`language` が 4 つの言語のどれでもない、conf が無いときはエラー(雛形でない)。
- `<name>` は §3 の規則(その言語の分も)を満たさなければならない。足す型名(Container では `_increment`・`_get` を付けたものも)とインスタンス名(型名 + `0`)が conf に既にあればエラー。足す識別子(型名と、それに `_manifest`・`_manif`・`_create`・`_entry_point`・`_manifest_entry_point` を付けたもの、Container の状態の型 `<Name>`、Haskell は `<Name>State`)がソースに識別子として既にあってもエラー。Component の名前と同じ Container は足してよい(`-t process` の雛形に Container `<name>` を足すとき)。
- 書き換えるのは **Component のソース 1 つと `app/app.conf` だけ**。新しいファイルは作らない(1 雛形 = 1 Component、§4)。issue #461 には「Rust はワークスペース、C++ は CMakeLists.txt、Haskell は .cabal」とあったが、同じ Component に足せばビルドのファイルを直す必要が無いので、この形にした(複数のクレートは §12 のとおり扱わない)。
- 足すコードは `templates/add/<lang>/<process|container>.tmpl`(§10.1)。変数は `name`(足す型名)と `Name`(その §3.1 の形)だけ。

言語ごとの足し方:

| 言語 | ソース | コードを入れる所 | 一覧に足す所 |
| --- | --- | --- | --- |
| Rust | `src/lib.rs` | `juiz_component_manifest!(` の行の前 | マクロの `processes = [ … ]`(1 行)の末尾、`containers = {` の閉じ `}` の前。無ければ `processes` は `);` の前、`containers` は `component_name` の次に作る |
| Python | `app/python/<component>.py` | ファイルの末尾(2 行空ける) | 一覧は無い。`from juiz import …`(1 行)に要るデコレータを足して並べ直す。その行が無ければ足すコードの頭に置く |
| C++ | `src/<component>.cpp` | Component のマニフェストの関数(`auto …() {` とその前のコメント)の前 | `return ComponentManifest(…)` の式の最後の `;` の前に `.add_process(<name>_manifest())`・`.add_container(…)` |
| Haskell | `src/<Component>.hs` | `juizComponent` の行の前 | `juizComponent` のリストの閉じ `]` の行の前に `, componentProcess …` など |

- app/app.conf: 最上位の `"processes":`・`"containers":` のブロックの末尾に、§5.1 と同じ形の項目(型名・インスタンス名、Container は `initial_value: 0` と ContainerProcess 2 つ)を足す。ブロックが無ければ §5.1 の順(processes → containers → ecs)の位置に作る。`"processes": []` は中身を足した形に直す。ほかの流れの形(`[ {…} ]`)はエラー。`ecs` には足さない。
- 書き換えた conf を読み直して、足した型が入っていることを確かめる。
- 入れる所が見つからない(利用者が形を変えた)ときは、何を探したかを示してエラーにし、手で足すよう案内する。
- 書き換えは、2 つのファイルの中身を全部作ってから書く。2 つめで失敗したら 1 つめを元に戻す(§8 と同じく、失敗したら何も変えない)。
- README.md の型の表(`types_table`)は直さない(表示で案内する)。
- 成功したら、足した型・書き換えたファイル・次に打つコマンド(ビルドのある言語は `scripts/dev.sh`、juiz の再起動、足した型の呼び出しの例)を表示する。Windows では `scripts\dev.ps1`(Haskell は `.sh`)。

## 3. 名前の規則

`<name>` は次をすべて満たさなければならない。

1. `[a-z][a-z0-9_]*` に合う。1〜64 文字(.juizapp のパッケージ名の上限、app-package.md §4.2)。
2. `juiz_sdk::identifier::validate_name` を通る(1 を満たせば必ず通るが、規則を一か所に寄せるため呼ぶ)。
3. `__` を含まず、`_` で終わらない(Haskell のモジュール名・cabal のパッケージ名に直すときに空の要素ができないように)。
4. 予約語でない。予約語は、Rust・Python・C++ のキーワード(`type`、`class`、`import`、`fn`、`int` など。名前はそのまま関数名・モジュール名になる)と、`juiz`・`juiz_sdk`・`pyjuiz`・`hsjuiz`・`cppjuiz`・`std`・`core`・`alloc`・`test`・`main`(依存先や標準のものと衝突する)。一覧は juiz_pkg の中に 1 つ持つ。
5. `-b haskell` のときだけ、`_` で区切った各要素が英字を 1 つ以上含む(cabal のパッケージ名の各要素は数字だけにできない。`arm_2` は不可)。
6. `-b cpp` のときだけ、`component`・`component_manifest`・`process_manifest`・`container_manifest`・`argument_manifest`・`topic_manifest` でない(Step 2)。cppjuiz は `ProcessManifest` などのクラスを名前空間の外に置いているので `<Name>` が衝突し、`DEFINE_COMPONENT_MANIFEST_ENTRY_POINT` が作る関数 `component_manifest_entry_point` は、Container `<name>` から `COMPONENT_CONTAINER_FACTORY` が作る `<name>_manifest_entry_point`・`<name>_entry_point` と衝突するため。

7. `-b haskell` のときだけ、GHC に付いてくるパッケージの名前(`base`・`text`・`containers`・`process`・`template_haskell` など)と base の最上位のモジュール(`prelude`・`numeric`・`foreign`)でない(Step 3)。`<name-kebab>` が cabal のパッケージ名として GHC のパッケージと衝突すると hsjuiz の依存が雛形そのものに解決されてしまい、`<Name>` のモジュール名が base のモジュールと衝突するため。一覧は juiz_pkg の `names::HASKELL_RESERVED`。

規則に合わないときは、どの規則に合わないかと、直した名前の例(大文字を小文字に、`-` を `_` に直したもの)をエラーに出す。

### 3.1 名前から作る名前

| 用途 | 形 | `my_robot` の例 |
| --- | --- | --- |
| Component 名、Rust のクレート名、Python のモジュール名、C++ のライブラリ名、cabal の foreign-library 名、.juizapp のパッケージ名 | `<name>` | `my_robot` |
| Rust・C++ の構造体・クラス名、Haskell のモジュール名 | `<Name>`(`_` で区切り、各要素の先頭を大文字にしてつなぐ) | `MyRobot` |
| cabal のパッケージ名、pyproject の project.name | `<name-kebab>`(`_` を `-` に) | `my-robot` |
| Container の型名 | `<name>` | `my_robot` |
| Process・ContainerProcess の型名 | `<name>_<動詞>` | `my_robot_add`、`my_robot_increment` |
| conf のインスタンス名 | 型名 + `0`(`juiz -1` と同じ付け方) | `my_robot0`、`my_robot_add0` |

## 4. テンプレートの種類

どの種類でも、作るのは **1 つの Component**(`plugins.components.<name>`)にする。種類で変わるのは、Component の中に入れる型だけ。conf の `plugins` の書き方と `juiz pack` の流れを種類によらず同じにし、Step 5 の `juiz pkg add` で型を足せるようにするため。

| 種類 | 入れる型 | 用途 |
| --- | --- | --- |
| `component`(既定) | `container` と `process` の両方 | 状態を持つ部品と、状態を持たない計算を 1 つに入れる。teleop_twist と同じ「1 プラグイン = 1 Component」 |
| `process` | Process `<name>_add` | 冪等な関数だけ |
| `container` | Container `<name>`、ContainerProcess `<name>_increment`・`<name>_get` | 状態を持つ部品 |

型の中身はどの言語でも同じにする(チュートリアル第 1・2 章と同じ働き)。

| 型 | 種類 | 引数(既定値) | 戻り値 | 働き |
| --- | --- | --- | --- | --- |
| `<name>_add` | Process | `a: int`(0)、`b: int`(0) | int | `a + b` を返す |
| `<name>` | Container | `initial_value: int`(0) | — | 整数 `value` を 1 つ持つ |
| `<name>_increment` | ContainerProcess | `step: int`(1) | int | `value += step` して `value` を返す |
| `<name>_get` | ContainerProcess(読むだけ) | なし | int | `value` を返す。readonly を書ける言語では readonly にする |

- 引数と戻り値には型を付ける(Rust は `i64`、Python は型注釈 `int`、C++ は `int64_t`、Haskell は `Int64`)。型を付けた雛形にしておけば、管理画面の呼び出しフォームや接続時の型検査がそのまま効く(datatype.md)。
- 各型には英語の description を付ける。
- コードのコメントは英語にする(生成物は利用者のもので、日本語の利用者に限らないため)。README も英語にする。

## 5. 共通の生成物

`-b rust -t component` の例。`<name>` は `my_robot`。言語ごとの違いは §6。

```
my_robot/
├─ Cargo.toml                ビルドのファイル(言語ごと、§6)
├─ src/lib.rs                Component のソース(言語ごと、§6)
├─ app/
│  ├─ app.conf               起動する conf。.juizapp の entry
│  └─ juizapp.yaml           .juizapp のマニフェスト
├─ scripts/
│  ├─ dev.sh   dev.ps1       ビルドしてプラグインを app/plugins/<ターゲット>/ に置く(Python には無い)
│  └─ pack.sh  pack.ps1      dist/ に <name>-0.1.0.juizapp を作る
├─ README.md
├─ .gitignore
└─ LICENSE-MIT  LICENSE-APACHE   (--license mit-apache のときだけ)
```

- `.ps1` は Windows で動かせる言語(Rust・Python・C++)にだけ置く。Haskell には置かない(Haskell の Windows は #137)。
- `.sh` は実行権限(0755)を付けて書く(Unix のみ。Windows では何もしない)。
- 改行は LF、文字コードは UTF-8(BOM なし)。
- git のリポジトリは作らない(`git init` しない)。

### 5.1 app/app.conf

`-t component` の例。`process` では `processes` だけ、`container` では `containers` だけを書く。

```yaml
# my_robot: start-up configuration of the juiz component my_robot
#   development: juiz -f app/app.conf -d
#                (Rust, C++ and Haskell: run scripts/dev.sh first to put the plugin in app/plugins/<target>/)
#   package:     scripts/pack.sh, then juiz run dist/my_robot-0.1.0.juizapp
"name": "my_robot"
"option":
  "http_broker":
    "start": true
    "port": 8000
"plugins":
  "components":
    "my_robot":
      "language": "rust"
      "path": "plugins/{target}"
"processes":
  - "type_name": "my_robot_add"
    "name": "my_robot_add0"
"containers":
  - "type_name": "my_robot"
    "name": "my_robot0"
    "initial_value": 0
    "processes":
      - "type_name": "my_robot_increment"
        "name": "my_robot_increment0"
      - "type_name": "my_robot_get"
        "name": "my_robot_get0"
"ecs":
  # Calls my_robot_increment0 periodically once started (auto_start: true to start it with juiz)
  - "type_name": "main_loop_ec"
    "name": "main_ec"
    "rate": 1
    "bind":
      - "type_name": "my_robot_increment"
        "name": "my_robot_increment0"
        "container_name": "my_robot0"
    "auto_start": false
```

- 先頭のコメントは言語によらず同じ文にする(§4 のとおり英語。言語ごとに conf のテンプレートを分けないため)。
- キーは teleop_twist と同じく `"` で囲み、順番もこの順にする(Step 5 の `juiz pkg add` が書き足す場所を決めやすくするため)。
- `language` と `path` は言語ごと: Rust `rust` / `plugins/{target}`、C++ `c++` / `plugins/{target}`、Haskell `haskell` / `plugins/{target}`、Python `python` / `python`。
- `ecs` は `container` と `component` だけに書き、`auto_start: false` にする(起動しただけで値が変わり続けないように。使い方の見本として置く)。
- ポートは 8000(チュートリアルと同じ)。

### 5.2 app/juizapp.yaml

```yaml
# .juizapp manifest (juiz docs/spec/app-package.md §4). languages, targets, rust and files are added by juiz pack
format: 1
name: my_robot
version: 0.1.0
kind: app
description: "my_robot: a juiz component"
license: MIT OR Apache-2.0        # --license mit-apache のときだけ
juiz: ">=0.2.5"                   # §7
entry: app.conf
```

- C++ では `juiz: "=<版>"` にする(Step 2)。C++ の .juizapp は libjuiz_sdk を同梱し(§6.3)、libjuiz_sdk はホストの juiz と版が一致しないと読み込めない(`juiz_sdk_build_info`)。`>=` のままだと、違う版の juiz で `juiz run` したときに読み込みの段で分かりにくく失敗するので、run の段 3(app-package.md §9)で版の違いと入れ方を示して止まるようにする。Haskell も同じ理由で `=` にする(Step 3。Haskell の .juizapp も libjuiz_sdk を同梱する、§6.4)。
- Python では次を足す。

```yaml
python:
  version: ">=3.10"
```

- パッケージの版 `version` は `0.1.0` から始める。ビルドのファイル(Cargo.toml・pyproject.toml・CMakeLists.txt・.cabal)の版も `0.1.0` にそろえる。版を上げるときにそろえるのは利用者の仕事とし、README にそう書く。

### 5.3 scripts/dev.sh・dev.ps1

ビルドして、プラグイン(と、C++・Haskell では隣に要るライブラリ)を `app/plugins/<ターゲット>/` に置く。

- `<ターゲット>` は `juiz pkg target` で得る(§2.3)。juiz が PATH に無ければ、入れ方を示して止める。
- 引数 `--release`(ps1 は `-Release`)で release ビルド。既定は debug。
- 最後に置いたファイルのパスを表示する。
- Rust では、環境変数 `CARGO_TARGET_DIR` があればそこからビルドの成果物を取る(無ければ `target/`)。juiz_pkg のテストは、これで雛形のビルドを `target/pkg-test` にまとめる。

### 5.4 scripts/pack.sh・pack.ps1

`dist/<name>-<version>.juizapp` を作る。

1. `--no-build`(ps1 は `-NoBuild`)が無ければ、`scripts/dev.sh --release` を呼ぶ(Python は何もしない)。
2. 一時ディレクトリに `app/` を写し、LICENSE-MIT・LICENSE-APACHE があれば一緒に写す(.juizapp にライセンスを入れるため。teleop_twist の pack.sh と同じ)。
3. Haskell では、一時ディレクトリの `plugins/<ターゲット>/` を `scripts/bundle-plugin-deps.sh` で依存ライブラリごと集め直す(§6.4)。
4. `juiz pack <一時ディレクトリ> -o dist` を呼ぶ。

Python の pack.sh にはビルドが無いので、`--no-build` は受け付けて無視する。

複数の OS のプラグインを 1 つの .juizapp に入れたいときは、それぞれの OS で `scripts/dev.sh --release` した `app/plugins/<ターゲット>/` を 1 か所に集め、`scripts/pack.sh --no-build` する(README に書く)。

### 5.5 README.md・.gitignore・ライセンス

- README.md(英語)には、何が入っているか(§4 の表)、要るもの(juiz の版、言語ごとの道具)、ビルド・起動・呼び出し・pack の手順(§9 の表示と同じもの)、版をそろえる注意(§7)を書く。
- .gitignore には、ビルドの出力(`/target`、`/build`、`/dist-newstyle`、`__pycache__/`、`/.venv`)、`/app/plugins/`、`/dist/`、`*.juizapp` を書く。
- `--license mit-apache` のとき: LICENSE-MIT(`Copyright (c) <年> <author>`)と LICENSE-APACHE を置き、Cargo.toml の `license`、pyproject.toml の `license`、.cabal の `license`、juizapp.yaml の `license` に `MIT OR Apache-2.0` を書く。`none` のときはどの欄も書かない(書いていない、と README に書くのでもなく、行ごと無い)。

## 6. 言語ごとの生成物

### 6.1 Rust

```
Cargo.toml
src/lib.rs
```

- Cargo.toml: `[package]` の name は `<name>`、version `0.1.0`、edition 2021、`publish = false`。`[lib] crate-type = ["cdylib", "rlib"]`(rlib は `cargo test` のため)。`[dependencies] juiz_sdk = "=<版>"`。ワークスペースにはしない(1 クレート)。
- src/lib.rs: `#[juiz_component_process]`・`#[juiz_component_container]`・`#[juiz_component_container_process]` と `juiz_component_manifest!` で書く(juiz_examples_rust の example_control と同じ書き方)。`-t component` の例:

```rust
//! my_robot: a juiz component.
use juiz_sdk::prelude::*;

/// State of the container `my_robot`.
#[repr(Rust)]
pub struct MyRobot {
    pub value: i64,
}

#[juiz_component_process(
    description = "Returns a + b."
    arguments = { default = { a = 0 b = 0 } }
)]
fn my_robot_add(a: i64, b: i64) -> JuizResult<i64> {
    Ok(a + b)
}

#[juiz_component_container(
    description = "Holds an integer value."
    arguments = { default = { initial_value = 0 } }
)]
fn my_robot(initial_value: i64) -> JuizResult<Box<MyRobot>> {
    Ok(Box::new(MyRobot { value: initial_value }))
}

#[juiz_component_container_process(
    container_type = "my_robot"
    description = "Adds step to the value and returns it."
    arguments = { default = { step = 1 } }
)]
fn my_robot_increment(container: &mut ContainerImpl<MyRobot>, step: i64) -> JuizResult<i64> {
    container.value += step;
    Ok(container.value)
}

#[juiz_component_container_process(
    container_type = "my_robot"
    description = "Returns the value."
)]
fn my_robot_get(container: &ContainerImpl<MyRobot>) -> JuizResult<i64> {
    Ok(container.value)
}

juiz_component_manifest!(
    component_name = "my_robot"
    containers = {
        my_robot = [ my_robot_increment, my_robot_get ]
    }
    processes = [ my_robot_add ]
);
```

  - Step 1 でビルドして確かめた: 戻り値に `i64` をそのまま書ける(マクロが `JuizResult<Capsule>` に直す、datatype.md)。Container を `&ContainerImpl<T>`(`mut` なし)で受けると readonly になる(`juiz container-process list --long` に `readonly` が出る)。上の例はそのまま使える。
  - `#[cfg(test)]` のテストを 1 つ置く(`my_robot_add` の結果を確かめる程度)。マクロが関数を `CapsuleMap` を受けるものに書き換えるので、テストは `CapsuleMap` に引数を入れて呼ぶ。`-t container` には Process が無いので置かない。
- dev.sh: `cargo build [--release]` → `target/<profile>/` の `lib<name>.so` / `lib<name>.dylib` / `<name>.dll` を `app/plugins/<ターゲット>/` に写す(teleop_twist の dev_plugin.sh と同じ)。
- 雛形の Rust プラグインは C ABI(rust-plugin-abi.md)なので、.juizapp の `juiz` は `>=<版>` でよい(app-package.md §4.3)。

### 6.2 Python

```
pyproject.toml
app/python/<name>.py
```

- Component のファイルは `app/python/<name>.py`。conf の `path` は `python`(app-package.md §4 の例と同じ置き方)。ビルドが無いので `scripts/dev.*` は置かない。
- pyproject.toml: project.name `<name-kebab>`、version `0.1.0`、`requires-python = ">=3.10"`、`dependencies = ["pyjuiz==<版>"]`、`[tool.setuptools] py-modules = []`(チュートリアル第 1 章と同じ形)。pip の juiz-cli は pyjuiz を同じ版で依存に持つので、juiz を cargo で入れた人にも pip で入れた人にも、この 1 行で足りる。
- `<name>.py`: `@juiz_process`・`@juiz_container`・`@juiz_container_process(container_type="<name>")` で書く(チュートリアル第 5 章 Python と同じ)。引数と戻り値に型注釈を付ける(型注釈は必須、PR #54)。Container の状態はクラス `<Name>` に持つ。`<name>_get` は `readonly=True` にする。description は docstring に書く(pyjuiz はデコレータの引数ではなく docstring を使う)。
- pyproject.toml の `[build-system]` は `setuptools>=77` にする(`--license mit-apache` の `license = "MIT OR Apache-2.0"`(PEP 639 の SPDX の書き方)を読めるのは 77 から。チュートリアルの `>=61` だと `pip install -e .` が失敗する)。
- README には venv を作って `pip install -e .` する手順を書く。

### 6.3 C++

```
CMakeLists.txt
src/<name>.cpp
```

- CMakeLists.txt はチュートリアル第 1 章 C++ の形を元にする。
  - `set(JUIZ_VERSION "<版>")`、`JUIZ_SDK_DIR` の既定は `--juiz-sdk-dir`(省けば `~/.juiz/sdk/${JUIZ_VERSION}`、Windows は `%USERPROFILE%`)。キャッシュ変数なので `cmake -D JUIZ_SDK_DIR=…` で変えられる。
  - `--juiz-sdk-dir` を省いたときは、CMakeLists.txt に絶対パスを書かない(`$ENV{HOME}`・`$ENV{USERPROFILE}` から作る。雛形を git で共有しても、ほかの人の機械でそのまま使えるように)。指定したときだけ、その絶対パスを `set(JUIZ_SDK_DEFAULT_DIR "…")` の 1 行で上書きする(変数 `juiz_sdk_dir_arg`、§10.2。省いたときは値が無いので行ごと消える)。
  - `find_package(juiz_sdk ${JUIZ_VERSION} EXACT CONFIG REQUIRED PATHS "${JUIZ_SDK_DIR}" NO_DEFAULT_PATH)`。
  - `add_library(<name> SHARED src/<name>.cpp)`、出力先 `build/plugins`(`$<1:…>` で構成名を足さない)、`juiz_sdk_bundle(<name>)`(app-package.md §12.2。rpath を `$ORIGIN` / `@loader_path` にし、libjuiz_sdk を隣に写す。Windows では juiz_sdk.dll を写す)。
- `src/<name>.cpp`: `COMPONENT_PROCESS_FACTORY`・`COMPONENT_CONTAINER_FACTORY`・`COMPONENT_CONTAINER_PROCESS_FACTORY` と `<name>_component_manifest()`・`DEFINE_COMPONENT_MANIFEST_ENTRY_POINT` で書く(チュートリアル第 5 章 C++ と同じ)。Container はクラス `<Name>`。
  - Component のマニフェストの関数は、チュートリアルの `component_manifest()` ではなく `<name>_component_manifest()` にする(Step 2)。各型の `<型名>_manifest()` はマクロが作るので、Container `<name>` が `component` などのときに衝突しないように。
  - マニフェストの変数は `<型名>_manif`(`static auto`)。引数は `add_int_arg` で名前・説明・既定値を書き、関数には同じ順に `int64_t` で渡る。戻り値は `std::optional<int64_t>`。
  - Container の関数は `juiz::CapsuleMap` で conf の Container の項目を受け取り、`initial_value` があれば使う(無ければ 0)。
  - `<name>_get` はマニフェストに `.readonly()` を付け、Container を `const <Name>*` で受け取る(transformations.md §14)。
- dev.sh: `cmake -S . -B build -D CMAKE_BUILD_TYPE=<Debug|Release>` → `cmake --build build --config <Debug|Release>` → `build/plugins/` のプラグインと libjuiz_sdk(Linux `lib<name>.so`・`libjuiz_sdk.so`、macOS `.dylib`、Windows `<name>.dll`・`juiz_sdk.dll`)を `app/plugins/<ターゲット>/` に写す。`build/plugins/` を丸ごと写さないのは、Windows でできる `.pdb`・`.exp`・`.lib` を入れないため。
  - 環境変数 `JUIZ_SDK_DIR` があれば `-D JUIZ_SDK_DIR=…` を付ける(`cmake` を自分で打たずに SDK の場所を変えるため)。cmake が PATH に無ければ、入れるものを示して止める。
  - pack.sh・pack.ps1 は Rust と同じもの(§5.4)。
- libjuiz_sdk の版は juiz と同じでなければならない(読み込み時に `juiz_sdk_build_info` で比べる)。README に `scripts/install-juiz-sdk.sh <版>` の手順を書く。

### 6.4 Haskell

```
cabal.project
<name-kebab>.cabal
src/<Name>.hs
scripts/bundle-plugin-deps.sh
```

- cabal.project: チュートリアル第 1 章 Haskell の形で、パスは**生成時に絶対パスで埋める**(チュートリアルで手で書き換えている所)。

```
packages: . <hsjuiz の絶対パス>
active-repositories: :none
package hsjuiz
  extra-lib-dirs: <libjuiz_sdk の lib の絶対パス>
  ghc-options: -optl-Wl,-rpath,<libjuiz_sdk の lib の絶対パス>
```

  - hsjuiz の場所は `--hsjuiz-dir`、省けば `~/juiz-<版>/bindings/hsjuiz`。lib の場所は `--juiz-sdk-dir` の `lib`、省けば `~/.juiz/sdk/<版>/lib`。`~` は生成時に展開する。
  - どちらも、生成時に無ければ警告を出し、用意するコマンド(`git clone --depth 1 --branch v<版> https://github.com/OpenJUIZ/juiz.git ~/juiz-<版>`、`install-juiz-sdk.sh <版>`)を表示する。生成は続ける(後から用意してよい)。
  - Hackage の hsjuiz(#360)が使えるようになったら、hsjuiz をソースから取らない形に変える(その時に決める)。
- `.cabal`: パッケージ名 `<name-kebab>`、version `0.1.0`、`foreign-library <name>`(type native-shared、`hs-source-dirs: src`、`other-modules: <Name>`)。
- `src/<Name>.hs`: `juizComponent` でまとめる(チュートリアル第 5 章 Haskell と同じ)。
- dev.sh: `cabal build all` → `dist-newstyle` の `lib<name>.so` / `lib<name>.dylib` を `app/plugins/<ターゲット>/` に写す(開発中は cabal.project の rpath で libjuiz_sdk が見つかる)。
- pack.sh: §5.4 の 3 で `scripts/bundle-plugin-deps.sh` を使い、libjuiz_sdk・GHC の共有ライブラリ・libffi・libgmp を集めて rpath を書き換える(app-package.md §12.2)。このスクリプトは juiz の `scripts/bundle-plugin-deps.sh` の写しをテンプレートに持ち、雛形に置く(同じ版のものになる)。写しが元と一致することを juiz_pkg のテストで確かめる。
- Windows では生成はするが、Windows の Haskell は対応していない(#137)と警告を出す。`.ps1` は置かない。

Step 3 で決めたこと:

- `src/<Name>.hs` の名前: 関数とマニフェストは `<name>_add`・`<name>_add_manifest`・`<name>_create`・`<name>_manifest`・`<name>_increment`・`<name>_get`・`<name>_component` のように `<name>_` で始める(Haskell のキーワード・Prelude・hsjuiz の名前と衝突しないように。C++ と同じ形)。Container の状態の型は `<Name>State`(`newtype <Name>State = <Name>State (IORef Int64)`)。`<Name>` のままだと `value` → `Value`、`maybe` → `Maybe` のように hsjuiz・Prelude の型と衝突するため。
- どの `-t` も `juizComponent` でまとめる(§4 の 1 つの Component)。引数は `addIntArg` で書き、Container の `initial_value` もマニフェストの引数として受け取る(conf の Container の項目から渡る)。`<name>_get` は `& readonly True`。
- `.cabal` は `foreign-library <name>` 1 つで、`common` は使わない。rpath は `$ORIGIN`(macOS は `@loader_path`)。`license-files` は書かない(`--license none` で行ごと消せないため。LICENSE は pack.sh が .juizapp に入れる)。
- dev.sh の `--release` は受け付けるだけ(cabal は既定で -O1。最適化を変えると hsjuiz まで作り直すので、変えたい人は cabal.project.local に書く)。開発中のプラグインは dist-newstyle の hsjuiz と cabal.project の rpath の libjuiz_sdk を使う。
- pack.sh はホストのターゲットの `app/plugins/<ターゲット>/` だけを `bundle-plugin-deps.sh` で集め直す。ほかのターゲットのディレクトリは、その OS で集めたものをそのまま入れる(README に手順)。Linux では patchelf が要る。
- hsjuiz(`<hsjuiz の場所>/hsjuiz.cabal`)・libjuiz_sdk(§9 の C++ と同じ判定)が無ければ、警告と次の手順に用意するコマンドを出す。hsjuiz は既定の場所なら `git clone --depth 1 --branch v<版> … ~/juiz-<版>`、`--hsjuiz-dir` が `…/bindings/hsjuiz` ならその 2 つ上に clone する形にする。README には在るかどうかによらず書く。
- Windows で動かしたときも、表示する手順は `.sh` の形にする(`.ps1` が無いため)。

### 6.5 `--idl`(型付きの雛形、Step 4)

`juiz pkg create <name> -b rust --idl <file.jidl>` は、§4 の例の型の代わりに、.jidl の `process` / `container` 宣言の型を入れた Component を作る。関数は `juiz idl -g rust-skeleton`(datatype.md §18)と同じもの(引数・戻り値が .jidl の型、本体は `todo!()`)で、juiz_idl の `rust_skeleton_items` で作る。

- 対象は Rust だけ。ほかの言語で `--idl` を付けるとエラーにする(skeleton の生成器が無いため。#468)。
- `-t` とは一緒に使えない(clap の `conflicts_with`)。§4 の例の型は入れない。
- 宣言のある名前空間は 1 つでなければならない(1 雛形 = 1 Component、§4)。宣言が無い・2 つ以上の名前空間にある(import した .jidl にも宣言がある)ときはエラー。juiz_std の名前空間は数えない。Component の名前は `<name>`(rust-skeleton の名前空間から作る名前ではない)。
- .jidl は `idl/` に写す。import している .jidl(と .avsc)も、渡した .jidl のディレクトリからの相対パスのまま `idl/` に写す。そのディレクトリの外のファイルを import していればエラー(写すと相対パスが壊れるため)。組み込みの `juiz_std/…` は写さない。
- 宣言の名前が Rust のキーワードなど、rust-skeleton が作れないものは、ファイルを書く前にエラーにする(§8)。

生成物(`-b rust -t component` との違い):

```
my_robot/
├─ Cargo.toml          [build-dependencies] juiz_idl を足す。juiz_std の型を使うときは juiz_std_types も
├─ build.rs            juiz_idl::build で .jidl の型を OUT_DIR/juiz_idl.rs に作る
├─ idl/<file>.jidl     渡した .jidl(と import している .jidl)の写し
├─ src/lib.rs          juiz_idl.rs を include! し、宣言ごとの関数(本体は todo!())と juiz_component_manifest!
├─ app/app.conf        宣言の型ごとにインスタンス(型名 + 0)を書く。ecs は書かない
└─ …                   app/juizapp.yaml・scripts/・README.md・.gitignore は §5 と同じ(README は --idl 用)
```

- juiz_std の型(名前空間が `juiz_std` か `juiz_std.` で始まる型)が読んだ型にあれば、Cargo.toml に `juiz_std_types = "=<版>"` を足し、build.rs に `.extern_path("juiz_std", "::juiz_std_types::juiz_std")` を書く(datatype.md §24.2。同じ型が 2 つの Rust の型にならないように)。無ければどちらも書かない。
- `juiz_idl` と `juiz_std_types` の版は juiz_sdk と同じ(§7.1)。`--juiz-source` ではソースの `juiz_idl`・`juiz_std_types` を指す。
- app.conf のキーの順番は §5.1 と同じ。Container の引数は書かない(rust-skeleton の Container は引数を持たない)。
- Container の状態は空の struct(JIDL では状態を型付けしない、datatype.md §18)。ContainerProcess は `&mut ContainerImpl<…>` で受ける(JIDL には readonly の書き方が無い)。
- src/lib.rs は juiz_idl の出力をそのまま入れるので、テンプレートの置換(§10.2)は通さない。先頭の説明は英語(§4)。rust-skeleton の Container の状態のコメントも、これに合わせて英語にした。
- `cargo test` 用のテストは置かない(本体が `todo!()` なので)。
- 生成後の表示(§9)は 1 行目の種類が `from idl/<file>.jidl` になり、`scripts/dev.sh` の前に「src/lib.rs の todo!() を書く」行が入る。呼び出しの例は宣言の型ごとに出し、引数は既定値、無ければ型から作った値(数は 0、文字列は ""、配列は []、record はフィールドごとに同じ規則か既定値、enum は既定の記号か最初の記号、`T?` は null)にする。本体を書くまでは呼び出すと失敗する、と README に書く。

## 7. 版の埋め方

### 7.1 版

雛形に書く juiz の版 `<版>` は、`juiz pkg` を動かした juiz 自身の版、つまり juiz_pkg の `env!("CARGO_PKG_VERSION")` にする(ワークスペースで juiz と同じ版)。

| ファイル | 書き方 | 理由 |
| --- | --- | --- |
| Cargo.toml | `juiz_sdk = "=<版>"` | C ABI なので厳密には不要だが、雛形を作った juiz とそろえておくと、マクロの書き方とホストの機能が食い違わない(teleop_twist と同じ) |
| pyproject.toml | `pyjuiz==<版>` | pyjuiz は juiz と同じ版で出している |
| CMakeLists.txt | `JUIZ_VERSION "<版>"` と `find_package(... EXACT)` | libjuiz_sdk はホストと版が一致しないと読み込めない |
| cabal.project | hsjuiz・libjuiz_sdk のパスに `<版>` が入る | 同上 |
| app/juizapp.yaml | `juiz: ">=<版>"`(C++・Haskell は `"=<版>"`、§5.2) | app-package.md §4.3 の pack の既定と同じ。C++・Haskell は同梱する libjuiz_sdk の版がホストと一致しないと読めないため |
| README.md | 要る juiz の版 | |

### 7.2 `--juiz-source`(開発者向け)

juiz 自身のテストと CI では、まだ公開していない版で雛形をビルドしたい。`--juiz-source <juiz のリポジトリ>` を付けると、公開版の代わりにソースを使う。

| 言語 | 変わるところ |
| --- | --- |
| Rust | `juiz_sdk = { path = "<dir>/juiz_sdk" }` |
| Haskell | hsjuiz の場所が `<dir>/bindings/hsjuiz`(`--hsjuiz-dir` より弱い) |
| Python | 変わらない(テストは先に pyjuiz をソースから入れておく) |
| C++ | 変わらない(テストは `--juiz-sdk-dir` か `-D JUIZ_SDK_DIR` でソースからビルドした SDK を指す) |

`<dir>` は絶対パスにして書く。README と生成後の表示には、ソースを使っていることを書く。

## 8. 作り先の規則

- 作り先(`-o`、省けば `./<name>`)が**ファイル**として在れば、エラーにする。
- **空でないディレクトリ**として在れば、エラーにする。上書きも、足りないファイルだけ足すこともしない(`--force` も設けない)。隠しファイルだけ(`.git` など)でも空でないものとして扱う。
- 空のディレクトリとして在れば、その中に作る(空のリポジトリを clone した後など)。
- 無ければ親ディレクトリごと作る。
- 書いている途中で失敗したら、作ったものを消して、作る前の状態に戻す(作り先を自分で作ったなら作り先ごと、空のディレクトリだったなら中身だけ)。
- 生成するファイルの一覧は先に全部決め、ファイルを 1 つも書かないうちに、名前・オプション・作り先の検査をすべて済ませる。

## 9. 生成後の表示

成功したら、作ったものと「次に打つコマンド」を表示する。`-b rust -t component`、名前 `my_robot` の例:

```
Created my_robot (rust, component) in ./my_robot  [juiz_sdk =0.2.5]

Next steps:
  cd my_robot
  scripts/dev.sh                  # build and put the plugin in app/plugins/<target>/
  juiz -f app/app.conf -d         # start (keep it running)

  # in another terminal
  juiz process call "http://127.0.0.1:8000/process/my_robot_add0::my_robot_add" '{"a": 1, "b": 2}' -p
  juiz container-process call "http://127.0.0.1:8000/container_process/my_robot_increment0::my_robot_increment:my_robot0" '{"step": 1}' -p

  scripts/pack.sh                 # make dist/my_robot-0.1.0.juizapp
  juiz run dist/my_robot-0.1.0.juizapp
```

- 言語ごとに先頭の手順が変わる: Python は `python3 -m venv .venv` → 有効化 → `pip install -e .`、C++ は SDK の用意(無ければ)→ `scripts/dev.sh`、Haskell は hsjuiz・SDK の用意(無ければ)→ `scripts/dev.sh`。
  - C++ の「無ければ」は、SDK の場所(`--juiz-sdk-dir`、省けば `~/.juiz/sdk/<版>`)に `lib/cmake/juiz_sdk/juiz_sdkConfig.cmake` が無いとき。用意するコマンドは `install-juiz-sdk.sh <版>`(Windows は `install-juiz-sdk.ps1`)で、`--juiz-sdk-dir` を付けたときはその場所を 2 つめの引数に付ける。README の手順(`next_steps`)には、在るかどうかによらず書く(Step 2)。
- Windows で動いているときは `scripts\dev.ps1` の形で表示する。
- 呼び出しの例は、そのテンプレートに入っている型の分だけ出す。
- 警告(§6.4 の hsjuiz が無いなど)は、この表示の前に出す。

## 10. テンプレートの置き場所と置換

### 10.1 置き場所

```
juiz_pkg/templates/
├─ common/_base/            どの言語・種類にも入れるもの(.gitignore の共通部分、LICENSE など)
├─ common/<template>/       種類ごと・言語によらないもの(app/app.conf)
├─ <lang>/_base/            言語ごと・種類によらないもの(ビルドのファイル、scripts/、README)
└─ <lang>/<template>/       言語と種類ごとのもの(Component のソース)

juiz_pkg/templates/add/<lang>/<process|container>.tmpl   juiz pkg add で足すコード(§2.4。create の層には入らない)
```

- `<lang>` は `rust` / `python` / `cpp` / `haskell`、`<template>` は `component` / `process` / `container`。`--idl` のときは `<template>` の代わりに `idl` の層(`common/idl/`・`rust/idl/`)を重ね、その後に .jidl の写しと src/lib.rs をコードで足す(§6.5)。
- 雛形は `common/_base` → `common/<template>` → `<lang>/_base` → `<lang>/<template>` の順に重ね、同じ出力パスは後のものが勝つ。
- テンプレートのファイル名は必ず `.tmpl` で終え、出力では `.tmpl` を外す(`Cargo.toml.tmpl` → `Cargo.toml`)。テンプレートの中に `Cargo.toml` という名前があると、`cargo package` が juiz_pkg のディレクトリの中の別のパッケージとみなして外してしまうため。
- パスの中にも変数を書いてよい(`src/{{name}}.cpp.tmpl`、`src/{{Name}}.hs.tmpl`)。
- LICENSE-MIT・LICENSE-APACHE は `--license mit-apache` のときだけ出力する(コードで振り分ける)。
- バイナリには build.rs で `templates/` の下を走査して `include_str!` の表を作って埋め込む(ネットワーク不要、生成物は juiz の版と必ず一致する)。テンプレートエンジン・ディレクトリ埋め込みのクレートの依存は足さない。

### 10.2 置換の書式

- `{{変数名}}` を値に置き換える。変数名は `[a-z_A-Z]+`。前後に空白は入れない。
- 知らない変数名、置き換え後に `{{` が残ること、はどちらも juiz_pkg のテストで落とす(テンプレートの書き間違いを出荷前に見つけるため)。実行時には起こらない。
- テンプレートに `{{` をそのまま書く必要は今は無いので、逃がし方は設けない(要るようになったら決める)。
- 値が「無し」になる変数(`license`、`author`)を含む**行は、行ごと消す**。`--license none` で Cargo.toml などの `license = "{{license}}"` の行を消すため。条件分岐・繰り返しの書式は設けない。

| 変数 | 値 |
| --- | --- |
| `name` | `<name>` |
| `Name` | `<Name>`(§3.1) |
| `name_kebab` | `<name-kebab>`(§3.1) |
| `juiz_version` | `<版>`(§7.1) |
| `plugin_language` | conf の `language`(`rust` / `python` / `c++` / `haskell`) |
| `plugin_path` | conf の `path`(`plugins/{target}` / `python`) |
| `juiz_sdk_dep` | Rust の依存の右辺(`"=<版>"`、`--juiz-source` なら `{ path = "…" }`) |
| `juiz_sdk_dir` | C++・Haskell の libjuiz_sdk の場所(§6.3・§6.4) |
| `juiz_sdk_dir_arg` | `--juiz-sdk-dir` に指定した場所(絶対パス)。省けば無し(C++ の CMakeLists.txt、§6.3) |
| `hsjuiz_dir` | Haskell の hsjuiz の場所(§6.4) |
| `license` | `MIT OR Apache-2.0`、`--license none` なら無し |
| `author` | `--author`、`--license none` なら無し |
| `year` | 生成した年(UTC) |
| `types_table` | 入っている型の表(§4 の表の、その種類の行だけ。Markdown)。README に使う |
| `next_steps` | §9 の「次に打つコマンド」(`cd` を除き、Unix の形)。README に使う |
| `juiz_idl_dep` | Rust の build-dependencies の juiz_idl の右辺(`juiz_sdk_dep` と同じ形、§6.5) |
| `juiz_std_types_dep` | juiz_std_types の右辺。`--idl` で juiz_std の型を使うときだけ、ほかは無し(§6.5) |
| `idl_main` | `--idl` の .jidl の写し(`idl/<file>.jidl`)。`--idl` でなければ無し |
| `idl_conf` | `--idl` の app.conf の `processes` と `containers`(複数行)。`--idl` でなければ無し |
| `idl_extern` | build.rs の `.extern_path(…)` の行。juiz_std の型を使うときだけ、ほかは無し |

`types_table` と `next_steps` は、種類・言語で変わる README の部分を、条件分岐の書式を設けずに 1 つのテンプレートで書くために足した(Step 1)。

conf の `{target}` は juiz の置換(app-package.md §5.2)で、この書式とは別。`{{` と `{` は重ならない。

## 11. テスト

各 Step で、次を通さなければならない。

- 名前の規則(§3)の合う例・合わない例と、§3.1 の変換。
- 置換(§10.2)の、知らない変数・残った `{{`・行ごと消す規則。全言語・全種類の組み合わせ(4 × 3)を生成して、`{{` が残らないこと。
- 作り先の規則(§8): ファイルが在る・空でないディレクトリ・空のディレクトリ・途中で失敗したときに何も残らないこと。
- 生成から配布まで(言語の Step ごと): 一時ディレクトリに `--juiz-source` で生成 → `scripts/dev.sh` → `juiz -f app/app.conf` で起動 → 入っている型を全部 call して §4 の値が返る → `scripts/pack.sh` → `juiz run` で起動できる。Rust・Python は Step 1、C++ は Step 2、Haskell は Step 3。CI では、その言語の道具が入っている OS で回す(Haskell は Linux・macOS)。
  - Step 2 の C++: ワークスペースの `cargo build` でできた libjuiz_sdk を `scripts/package-juiz-sdk.sh` で配布と同じアーカイブにして一時ディレクトリに展開し、`--juiz-sdk-dir` で指す(`find_package(… EXACT)` と `juiz_sdk_bundle` を配布物と同じ形で通すため)。ポートは 8077。.juizapp に libjuiz_sdk が入っていることと、`juiz: "=<版>"` になっていることも確かめる。cmake と C++ コンパイラが要る(scripts/test.sh と同じ)。
  - Step 1 の形(juiz_app/tests/pkg_cli_test.rs): Unix で回す(`.sh` を使うため。`.ps1` は自動のテストが無い)。ほかのテストとポートが重ならないよう、生成した conf のポートを 8078(Rust)・8079(Python)に書き換えてから起動する。`scripts/dev.sh` でビルドした後なので、pack は `scripts/pack.sh --no-build` で release ビルドを省く。Python は `PYTHONPATH` の bindings/pyjuiz を使う(scripts/test.sh が設定する)。
- `--idl`(Step 4): juiz_pkg の単体テストで、生成するファイルの一覧・置換が残らないこと・juiz_std を使う/使わないときの Cargo.toml と build.rs・import した .jidl の写し・エラー(Rust のほかの言語、`-t` と一緒、宣言なし、2 つの名前空間、ディレクトリの外の import、Rust のキーワード)。pkg_cli_test では、juiz_std の型と別ファイルの record を使う .jidl から作り、`todo!()` の本体を書き換えてから dev.sh → 起動 → call → `juiz type list --long` に .jidl の型が出る → pack.sh → `juiz run` を通す(Unix、ポート 8082)。
- `juiz pkg add`(Step 5): juiz_pkg の単体テストで、4 言語 × 3 種類の雛形すべてに Process と Container を足せて(`{{` が残らず、元の型も残る)、各言語の一覧の書き換え・conf のブロックの位置(無いときに作る位置を含む)・エラー(conf に既にある、ソースの識別子と衝突、雛形でない、入れる所が無い)・2 つめの書き込みで失敗したときに元に戻ること。pkg_cli_test の Rust・Python・C++・Haskell の生成から配布までのテストでは、生成の直後に `juiz pkg add process <name>_sum` と `juiz pkg add container <name>_arm` を打ち(同じ名前をもう一度足すとエラーで conf が変わらないことも確かめる)、起動と `juiz run` の後に足した型も呼ぶ。
- Haskell: テンプレートの `bundle-plugin-deps.sh` が juiz の `scripts/bundle-plugin-deps.sh` と一致すること。
  - Step 3 の Haskell(pkg_cli_test): C++ と同じく配布と同じ形の libjuiz_sdk を `--juiz-sdk-dir` で指し、hsjuiz は `--juiz-source` の `bindings/hsjuiz`。ポートは 8076。.juizapp に libjuiz_sdk・hsjuiz・GHC の RTS が入っていることと `juiz: "=<版>"` を確かめる。ghc・cabal(Linux では patchelf も)が無ければ飛ばし、`JUIZ_REQUIRE_HASKELL=1` なら落とす。cargo test が LD_LIBRARY_PATH に足す `target/` の下は、scripts を呼ぶときに外す(残すと bundle-plugin-deps.sh の確かめで、隣に写した libjuiz_sdk より先に target/debug のものが見つかって失敗する)。

## 12. 後回しにするもの

| もの | 今の扱い | 後で足すなら |
| --- | --- | --- |
| `--idl` の Python・C++・Haskell | エラー(Step 4 は Rust だけ、§6.5) | #468。juiz_idl に各言語の skeleton の生成器を足してから |
| 外部のテンプレート(git・cargo-generate 互換) | しない | テンプレートの置き場所を指定するオプション |
| 基礎パッケージ(`kind: base`)・`depends` の雛形 | しない(`kind: app` だけ) | `--kind base` など |
| 複数クレート・複数 Component のワークスペース | しない(1 雛形 = 1 Component) | |
| Haskell の Windows | 生成するが警告 | #137 の後に `.ps1` と DLL の作り方を足す |
| CI のワークフロー(.github/workflows)の雛形 | 置かない | 置く場合は `${{ }}` と置換の書式の衝突を決める |

## 13. Step との対応

| Step | issue | この文書の節 |
| --- | --- | --- |
| 0 仕様書 | #456 | 全体 |
| 1 juiz_pkg クレート、`juiz pkg create`・`juiz pkg target`・`juiz-pkg`、Rust と Python | #457 | §2、§3、§4、§5、§6.1、§6.2、§7、§8、§9、§10、§11 |
| 2 C++ | #458 | §6.3 |
| 3 Haskell | #459 | §6.4 |
| 4 `--idl` | #460 | §6.5 |
| 5 `juiz pkg add`(任意) | #461 | §2.4、§5.1 |
| 6 サイトとリリース | #462、OpenJUIZ.github.io#87 | — |
