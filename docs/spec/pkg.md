# 雛形生成コマンド(juiz pkg)仕様

- 状態: 確定(juiz pkg Step 0、issue #456、2026-11-09)。Step 1(#457、2026-11-10)で Rust・Python を実装し、実装に合わせて §2.2・§5・§6.1・§6.2・§10.2・§11 を書き足した。Step 2(#458、2026-11-10)で C++ を実装し、§2.2・§3・§5.2・§6.3・§7.1・§9・§10.2・§11 を書き足した。Step 4(#460、2026-11-10)で `--idl` を Rust だけ実装し、§2.2・§6.5・§7・§9・§10・§11・§12 を書き足した。Step 5(#461、2026-11-10)で `juiz pkg add` を実装し、§1・§2.4・§10.1・§11・§12 を書き足した。#468(2026-11-11)で `--idl` を Python・C++・Haskell に広げ、§2.2・§6.5・§9・§10・§11・§12 を書き直した。Zig Step 5(#577、2026-11-22)で Zig を足し、§1・§2・§3・§5・§6.5・§6.6・§7・§10・§11・§12 を書き足した。設計文書 Step 0(#593、2026-11-23)で、設計文書 `juizdesign.yaml` を経由する 3 つのモード(§2.2・§2.5〜§2.7)を足し、§1・§2.4・§4・§5・§13 を書き足した(文書そのものの仕様は [design.md](design.md))。設計文書 Step 1(#594、2026-11-23)で `-t` の雛形に文書を置くことと `juiz pkg design`・`design check` を実装し、§2.2・§2.6・§2.7・§10・§11 を書き足した
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
- 対象の言語は Rust・Python・C++・Haskell・Zig(Zig は #577)。

雛形は Component の設計文書 `juizdesign.yaml`([design.md](design.md))を経由して作る(design.md の Step 1〜4 で順に切り替える)。モードは 3 つ:

| モード | コマンド | 節 |
| --- | --- | --- |
| ① 雛形 + 設計文書(今まで通り) | `juiz pkg create <name> -b <lang> [-t … \| --idl …]` | §2.2 |
| ② 設計文書から雛形 | `juiz pkg create -d <juizdesign.yaml> [-b <lang>]` | §2.5 |
| ③ 設計文書だけ | `juiz pkg design <name> [-t … \| --idl …]`、検査は `juiz pkg design check <file>` | §2.6・§2.7 |

扱わないもの(§12 で後回しにしたもの)は、外部のテンプレート(git から取るもの、cargo-generate 互換)、既存の雛形への型の追加(`juiz pkg add`、§2.4)以外の書き換え、Haskell と Zig の Windows。

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
                [--hsjuiz-dir <dir>] [--zigjuiz-dir <dir>] [--juiz-sdk-dir <dir>] [--juiz-source <dir>]
```

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `<name>` | (必須) | 名前。§3 の規則に合わなければエラー |
| `-b`, `--lang <lang>` | (必須) | `rust` / `python` / `cpp` / `haskell` / `zig`。`c++` も `cpp` として受ける。大文字小文字は区別しない |
| `-t`, `--template <template>` | `component` | `component` / `process` / `container`(§4) |
| `-d`, `--design <file>` | なし | 設計文書から雛形を作る(§2.5)。`<name>`・`-t`・`--idl` とは一緒に使えない |
| `--idl <file.jidl>` | なし | §4 の例の型の代わりに、.jidl の `process` / `container` 宣言から型付きの関数を作る(§6.5)。5 つの言語すべて(Step 4 で Rust、#468 で Python・C++・Haskell、#577 で Zig)。`-t` とは一緒に使えない |
| `-o`, `--output <dir>` | `./<name>` | 雛形を作るディレクトリそのもの(親ではない)。§8 の規則で既にあれば止める |
| `--license <kind>` | `none` | `mit-apache` で LICENSE-MIT・LICENSE-APACHE を置き、各ファイルの license の欄を `MIT OR Apache-2.0` にする(§5.5)。`none` では置かない |
| `--author <name>` | `git config user.name` | `--license mit-apache` のときの著作権者。省いて git からも取れなければエラー |
| `--hsjuiz-dir <dir>` | `~/juiz-<版>/bindings/hsjuiz` | Haskell のみ。cabal.project に書く hsjuiz のソースの場所(§6.4) |
| `--zigjuiz-dir <dir>` | なし(Releases の zigjuiz を `zig fetch` で取る) | Zig のみ。build.zig.zon に相対パスで書く zigjuiz のソースの場所(§6.6) |
| `--juiz-sdk-dir <dir>` | `~/.juiz/sdk/<版>` | C++・Haskell・Zig のみ。libjuiz_sdk を展開した場所(§6.3・§6.4・§6.6) |
| `--juiz-source <dir>` | なし | 開発者向け。juiz のリポジトリのソースを使う(§7.2)。juiz 自身のテストと CI で使う |

- 言語・テンプレートの値が違えば、受け付ける値を並べてエラーにする。
- 5 つの言語すべてにテンプレートがある(Step 3 で Haskell を足して、テンプレートの無い言語のエラーは無くした。Zig は #577)。
- 言語に関係しないオプション(`-b rust` に `--hsjuiz-dir` など)は、警告を出して無視する。
- 成功したら終了コード 0、失敗したら 0 以外。失敗したときは何も残さない(§8)。
- 表示は英語(juiz の CLI のほかの表示に合わせる)。
- 雛形の直下に、作った型を書いた設計文書 `juizdesign.yaml`(design.md)を置く。`-t` では種類のプリセット(design.md §7)、`--idl` では .jidl から作った文書(design.md §6.2)。`-t` は設計文書 Step 1(#594)から置き、Step 2・3(#595・#596)で生成も文書経由に切り替える。`--idl` は Step 4(#597)から(それまでは `--idl` の雛形に文書を置かない)。
- 置く文書の `component.language` には `-b` の言語を書く(その文書を `create -d` に渡したときの既定の言語になり、`design check` が言語の規則も確かめる)。文書はプリセットに言語を足したもので、design.md §9 の形。

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

- 言語と Component の名前は `app/app.conf` の `plugins.components` から読む。Component が 1 つでない、`language` が 5 つの言語のどれでもない、conf が無いときはエラー(雛形でない)。
- `<name>` は §3 の規則(その言語の分も)を満たさなければならない。足す型名(Container では `_increment`・`_get` を付けたものも)とインスタンス名(型名 + `0`)が conf に既にあればエラー。足す識別子(型名と、それに `_manifest`・`_manif`・`_create`・`_entry_point`・`_manifest_entry_point` を付けたもの、Container の状態の型 `<Name>`、Haskell・Zig は `<Name>State`)がソースに識別子として既にあってもエラー。Component の名前と同じ Container は足してよい(`-t process` の雛形に Container `<name>` を足すとき)。
- 書き換えるのは **Component のソース 1 つと `app/app.conf` だけ**。新しいファイルは作らない(1 雛形 = 1 Component、§4)。issue #461 には「Rust はワークスペース、C++ は CMakeLists.txt、Haskell は .cabal」とあったが、同じ Component に足せばビルドのファイルを直す必要が無いので、この形にした(複数のクレートは §12 のとおり扱わない)。
- 足すコードは `templates/add/<lang>/<process|container>.tmpl`(§10.1)。変数は `name`(足す型名)と `Name`(その §3.1 の形)だけ。

言語ごとの足し方:

| 言語 | ソース | コードを入れる所 | 一覧に足す所 |
| --- | --- | --- | --- |
| Rust | `src/lib.rs` | `juiz_component_manifest!(` の行の前 | マクロの `processes = [ … ]`(1 行)の末尾、`containers = {` の閉じ `}` の前。無ければ `processes` は `);` の前、`containers` は `component_name` の次に作る |
| Python | `app/python/<component>.py` | ファイルの末尾(2 行空ける) | 一覧は無い。`from juiz import …`(1 行)に要るデコレータを足して並べ直す。その行が無ければ足すコードの頭に置く |
| C++ | `src/<component>.cpp` | Component のマニフェストの関数(`auto …() {` とその前のコメント)の前 | `return ComponentManifest(…)` の式の最後の `;` の前に `.add_process(<name>_manifest())`・`.add_container(…)` |
| Haskell | `src/<Component>.hs` | `juizComponent` の行の前 | `juizComponent` のリストの閉じ `]` の行の前に `, componentProcess …` など |
| Zig | `src/<component>.zig` | `juiz.exportComponent(` を呼ぶ `comptime {`(とその前のコメント)の前 | `.parts = .{` を閉じる `},`(`.parts` と同じ字下げ)の行の前に `templates/add/zig/<process\|container>_part.tmpl` の要素。`const std = @import("std");` が無ければ(`--idl` の雛形)、`const juiz = @import("juiz");` の前に足す |

- app/app.conf: 最上位の `"processes":`・`"containers":` のブロックの末尾に、§5.1 と同じ形の項目(型名・インスタンス名、Container は `initial_value: 0` と ContainerProcess 2 つ)を足す。ブロックが無ければ §5.1 の順(processes → containers → ecs)の位置に作る。`"processes": []` は中身を足した形に直す。ほかの流れの形(`[ {…} ]`)はエラー。`ecs` には足さない。
- 書き換えた conf を読み直して、足した型が入っていることを確かめる。
- 入れる所が見つからない(利用者が形を変えた)ときは、何を探したかを示してエラーにし、手で足すよう案内する。
- 書き換えは、2 つのファイルの中身を全部作ってから書く。2 つめで失敗したら 1 つめを元に戻す(§8 と同じく、失敗したら何も変えない)。
- README.md の型の表(`types_table`)は直さない(表示で案内する)。
- 設計文書 Step 4(#597)から、雛形の直下の `juizdesign.yaml` にも足す型を書き足す(design.md §8.3)。`juizdesign.yaml` が無い雛形では警告して、ソースと conf だけを書き換える。
- 成功したら、足した型・書き換えたファイル・次に打つコマンド(ビルドのある言語は `scripts/dev.sh`、juiz の再起動、足した型の呼び出しの例)を表示する。Windows では `scripts\dev.ps1`(Haskell は `.sh`)。

### 2.5 `juiz pkg create -d`(設計文書から雛形、設計文書 Step 2・3)

```
juiz pkg create -d <juizdesign.yaml> [-b <lang>] [-o <dir>]
                [--license <mit-apache|none>] [--author <name>]
                [--hsjuiz-dir <dir>] [--zigjuiz-dir <dir>] [--juiz-sdk-dir <dir>] [--juiz-source <dir>]
```

- 設計文書の型を入れた Component を作る。関数は宣言・マクロ・manifest まで文書どおりで、本体は未実装の印(design.md §1)。
- 設計文書 Step 2(#595)で Rust・Python を作れるようにした。C++・Haskell・Zig は Step 3(#596)まで、言語が決まった所でエラーにする。
- 名前は文書の `component.name`。`<name>`・`-t`・`--idl` とは一緒に使えない(clap の `conflicts_with`。`<name>` は `-d` が無いときだけ必須)。
- 言語は `-b`、無ければ文書の `component.language`、どちらも無ければエラー。違えば警告して `-b` を使う(design.md §8.2)。
- `-o` を省けば `./<component.name>`。作り先が文書のあるディレクトリそのもののときの扱いは design.md §8.2。
- 書く前に design.md §5 の検査を通す(言語の規則も)。エラーがあれば何も書かない。
- 生成物は `--idl`(§6.5)と同じ形にそろえる。`idl:` の .jidl は文書からの相対パスのまま写す(`--idl` の `idl/<file>.jidl` の代わり)。app.conf は design.md §6.3、README の型の表と生成後の表示は design.md §6.4。
- 生成後の表示(§9)の 1 行目の種類は `from <文書のファイル名>`。「本体を書く」行は §6.5 と同じ。

### 2.6 `juiz pkg design`(設計文書だけ、設計文書 Step 1・4)

```
juiz pkg design <name> [-b <lang>] [-t <template> | --idl <file.jidl>] [-o <file>]
```

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `<name>` | (必須) | Component の名前(§3。`-b` を付ければその言語の規則も) |
| `-b`, `--lang <lang>` | なし | 文書の `component.language` に書く。付ければ言語の規則でも検査する |
| `-t`, `--template <template>` | `component` | プリセット(design.md §7) |
| `--idl <file.jidl>` | なし | .jidl の宣言から文書を作る(design.md §6.2・§8.4、Step 4)。`-t` とは一緒に使えない |
| `-o`, `--output <file>` | `./juizdesign.yaml` | 書くファイル |

- 雛形は作らず、設計文書を 1 つ書く。利用者が書き直して `create -d` に渡す使い方。
- 書き先がファイルとして在ればエラー(上書きしない、§8)。親ディレクトリが無ければ作る。
- 文書の書き方は design.md §9。書く前に design.md §5 の検査を通す。
- 表示は、書いたファイルと型の数、次に打つコマンド(`juiz pkg design check <file>`、`juiz pkg create -d <file>`)。`create -d` の行は、それを実装した Step 2 から出す(Step 1 は `check` の行だけだった)。`-b` を付けなかったときは、どちらにも `-b <rust|python|cpp|haskell|zig>` を付けて案内する。
- 書く前の検査で `-b` が無いときの W3(design.md §5.4)は出さない(プリセットなので)。
- `check` は §2.7 のサブコマンドなので、`juiz pkg design check` は名前 `check` の文書にはならない(名前 `check` の文書は書けない)。

### 2.7 `juiz pkg design check`(設計文書 Step 1)

```
juiz pkg design check <file> [-b <lang>]
```

- 設計文書を design.md §5 で検査する。言語は `-b`、無ければ文書の `component.language`、どちらも無ければ言語によらない検査だけ(design.md §8.1)。
- 通れば 1 行(`ok: <file> (component <name>: <n> processes, <n> containers, <n> container processes)`)と警告を出して終了コード 0。エラーがあれば、すべてのエラーを `<file>:<行>:<桁>: error: <道筋>: <文>` の形で出して 0 以外。
  - `ok:` の行は標準出力、エラーと警告(`<file>:<行>:<桁>: warning: …`)は標準エラー出力。エラーのときは最後に `error: <file> is not a valid design document (<n> errors)` を出す。型の数は 1 のとき単数形(`1 process`)。
  - 文書全体のこと(F4・W3 など)は道筋を書かない(`<file>: warning: <文>`)。位置が分からないとき(YAML の文書が 2 つあるなど)は `<file>: error: …`。
- ファイルは何も書かない。

## 3. 名前の規則

`<name>` は次をすべて満たさなければならない。

1. `[a-z][a-z0-9_]*` に合う。1〜64 文字(.juizapp のパッケージ名の上限、app-package.md §4.2)。
2. `juiz_sdk::identifier::validate_name` を通る(1 を満たせば必ず通るが、規則を一か所に寄せるため呼ぶ)。
3. `__` を含まず、`_` で終わらない(Haskell のモジュール名・cabal のパッケージ名に直すときに空の要素ができないように)。
4. 予約語でない。予約語は、Rust・Python・C++ のキーワード(`type`、`class`、`import`、`fn`、`int` など。名前はそのまま関数名・モジュール名になる)と、`juiz`・`juiz_sdk`・`pyjuiz`・`hsjuiz`・`cppjuiz`・`std`・`core`・`alloc`・`test`・`main`(依存先や標準のものと衝突する)。一覧は juiz_pkg の中に 1 つ持つ。
5. `-b haskell` のときだけ、`_` で区切った各要素が英字を 1 つ以上含む(cabal のパッケージ名の各要素は数字だけにできない。`arm_2` は不可)。
6. `-b cpp` のときだけ、`component`・`component_manifest`・`process_manifest`・`container_manifest`・`argument_manifest`・`topic_manifest` でない(Step 2)。cppjuiz は `ProcessManifest` などのクラスを名前空間の外に置いているので `<Name>` が衝突し、`DEFINE_COMPONENT_MANIFEST_ENTRY_POINT` が作る関数 `component_manifest_entry_point` は、Container `<name>` から `COMPONENT_CONTAINER_FACTORY` が作る `<name>_manifest_entry_point`・`<name>_entry_point` と衝突するため。

7. `-b haskell` のときだけ、GHC に付いてくるパッケージの名前(`base`・`text`・`containers`・`process`・`template_haskell` など)と base の最上位のモジュール(`prelude`・`numeric`・`foreign`)でない(Step 3)。`<name-kebab>` が cabal のパッケージ名として GHC のパッケージと衝突すると hsjuiz の依存が雛形そのものに解決されてしまい、`<Name>` のモジュール名が base のモジュールと衝突するため。一覧は juiz_pkg の `names::HASKELL_RESERVED`。

8. `-b zig` のときだけ、Zig のキーワード(`var`・`error`・`comptime` など、1〜4 の予約語に無いもの)、宣言に使えない基本の型・値の名前(`i8`・`u64` のような整数の型、`f64`・`usize`・`anyerror`・`undefined` など)、`zigjuiz`・`juiz_types`・`builtin`・`root` でない。`create` では 32 文字まで(build.zig.zon の `.name` の上限)。`<name>` は関数名と build.zig.zon の `.name`(識別子)になるため(#577)。一覧は juiz_pkg の `names::ZIG_RESERVED`。

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
- 3 つの種類は、組み込みの設計文書(プリセット)と言語ごとの本体の断片で表す(design.md §7)。プリセットの整数は `long`(JIDL の `int` は 32 bit のため)。

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
├─ juizdesign.yaml          設計文書(design.md、設計文書 Step 1 から)
├─ README.md
├─ .gitignore
└─ LICENSE-MIT  LICENSE-APACHE   (--license mit-apache のときだけ)
```

- `.ps1` は Windows で動かせる言語(Rust・Python・C++・Zig)にだけ置く。Haskell には置かない(Haskell の Windows は #137。Zig は #588 で足した)。
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
- `language` と `path` は言語ごと: Rust `rust` / `plugins/{target}`、C++ `c++` / `plugins/{target}`、Haskell `haskell` / `plugins/{target}`、Zig `zig` / `plugins/{target}`、Python `python` / `python`。
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

- C++ では `juiz: "=<版>"` にする(Step 2)。C++ の .juizapp は libjuiz_sdk を同梱し(§6.3)、libjuiz_sdk はホストの juiz と版が一致しないと読み込めない(`juiz_sdk_build_info`)。`>=` のままだと、違う版の juiz で `juiz run` したときに読み込みの段で分かりにくく失敗するので、run の段 3(app-package.md §9)で版の違いと入れ方を示して止まるようにする。Haskell も同じ理由で `=` にする(Step 3。Haskell の .juizapp も libjuiz_sdk を同梱する、§6.4)。Zig も同じ(#577、§6.6)。
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
    typed = "all"
    description = "Returns a + b."
    arguments = { default = { a = 0 b = 0 } }
)]
fn my_robot_add(a: i64, b: i64) -> JuizResult<i64> {
    Ok(a + b)
}

#[juiz_component_container(
    typed = "all"
    description = "Holds an integer value."
    arguments = { default = { initial_value = 0 } }
)]
fn my_robot(initial_value: i64) -> JuizResult<Box<MyRobot>> {
    Ok(Box::new(MyRobot { value: initial_value }))
}

#[juiz_component_container_process(
    container_type = "my_robot"
    typed = "all"
    description = "Adds step to the value and returns it."
    arguments = { default = { step = 1 } }
)]
fn my_robot_increment(container: &mut ContainerImpl<MyRobot>, step: i64) -> JuizResult<i64> {
    container.value += step;
    Ok(container.value)
}

#[juiz_component_container_process(
    container_type = "my_robot"
    typed = "all"
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

  - 設計文書 Step 2(#595)から、このソースはプリセットの設計文書と本体の断片から juiz_idl の `rust_design_items` で作る(design.md §7)。`typed = "all"` はそのときに付いた(引数・戻り値の型が manifest の `schema` に入る)。
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
- libjuiz_sdk の版は juiz と同じでなければならない(読み込み時に `juiz_sdk_build_info` で比べる)。README に `juiz sdk install` の手順を書く(juiz#548。それ以前は `scripts/install-juiz-sdk.sh <版>`)。

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
  - どちらも、生成時に無ければ警告を出し、用意するコマンド(`git clone --depth 1 --branch v<版> https://github.com/OpenJUIZ/juiz.git ~/juiz-<版>`、`juiz sdk install`)を表示する。生成は続ける(後から用意してよい)。
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

### 6.5 `--idl`(型付きの雛形、Step 4・#468)

`juiz pkg create <name> -b <lang> --idl <file.jidl>` は、§4 の例の型の代わりに、.jidl の `process` / `container` 宣言の型を入れた Component を作る。関数は `juiz idl -g <lang>-skeleton`(datatype.md §18・§20.6・§21.6・§22.4)と同じもの(引数・戻り値が .jidl の型、本体は未実装)で、juiz_idl の `rust_skeleton_items`・`python_skeleton_items`・`cpp_skeleton_items`・`haskell_skeleton_items` で作る。Step 4(#460)は Rust だけで、#468 で Python・C++・Haskell を足した。

共通の規則:

- `-t` とは一緒に使えない(clap の `conflicts_with`)。§4 の例の型は入れない。
- 宣言のある名前空間は 1 つでなければならない(1 雛形 = 1 Component、§4)。宣言が無い・2 つ以上の名前空間にある(import した .jidl にも宣言がある)ときはエラー。juiz_std の名前空間は数えない。Component の名前は `<name>`(skeleton の名前空間から作る名前ではない)。
- .jidl は `idl/` に写す。import している .jidl(と .avsc)も、渡した .jidl のディレクトリからの相対パスのまま `idl/` に写す。そのディレクトリの外のファイルを import していればエラー(写すと相対パスが壊れるため)。組み込みの `juiz_std/…` は写さない。
- その言語の skeleton が作れない宣言(キーワードの名前、Python・C++ の `T?`・`bytes` の引数、Python で既定値のある引数の後ろの既定値の無い record など。datatype.md の各節)と、その言語で型のコードが作れない .jidl(C++・Haskell の名前空間の循環など)は、ファイルを書く前にエラーにする(§8)。
- Component のソースは juiz_idl の出力に英語の先頭の説明を付けてそのまま入れるので、テンプレートの置換(§10.2)は通さない。
- app/app.conf は宣言の型ごとにインスタンス(型名 + 0)を書く。キーの順番は §5.1 と同じ、ecs は書かない。Container の引数は書かない(skeleton の Container は引数を持たない)。
- Container の状態は空(JIDL では状態を型付けしない、datatype.md §18)。ContainerProcess は readonly にしない(JIDL には readonly の書き方が無い)。
- テスト用のコード(Rust の `cargo test` など)は置かない(本体が未実装なので)。
- README は言語ごとの `--idl` 用(`<lang>/idl/README.md.tmpl`)。本体を書くまでは呼び出すと失敗すること、.jidl を変えたときにすること(型の作り直しと、`juiz idl -g <lang>-skeleton` で関数を書き出し直して写すこと)を書く。
- 生成後の表示(§9)は 1 行目の種類が `from idl/<file>.jidl` になり、「本体を書く」行(`# write the bodies of the functions (<未実装の印>) in <ソース>, then`)が入る。ビルドのある言語は `scripts/dev.sh` の前、Python は `pip install -e .` の後。呼び出しの例は宣言の型ごとに出し、引数は既定値、無ければ型から作った値(数は 0、文字列は ""、配列は []、record はフィールドごとに同じ規則か既定値、enum は既定の記号か最初の記号、`T?`・`image`・`any` は null)にする。
- 作った雛形にも `juiz pkg add`(§2.4)で型を足せる(ソースの形を §2.4 の入れる所に合わせてある)。

言語ごとの生成物(`-t component` との違い):

| 言語 | Component のソース(本体の未実装の印) | .jidl の型のコード | ビルドのファイル |
| --- | --- | --- | --- |
| Rust | `src/lib.rs`(`todo!()`)。`juiz_idl.rs` を `include!` する | build.rs が `juiz_idl::build` で OUT_DIR に作る | Cargo.toml に `[build-dependencies] juiz_idl`、juiz_std の型を使うときは `juiz_std_types` も |
| Python | `app/python/<name>.py`(`raise NotImplementedError`) | **作るときに** `juiz idl -g python --extern juiz_std` と同じものを `app/python/<名前空間>/…/__init__.py` に書く(juiz の Python プラグインは .py のディレクトリを `sys.path` に入れるので、そのまま import できる)。juiz_std は pyjuiz に入っているので書かない | 変わらない(pyproject.toml) |
| C++ | `src/<name>.cpp`(`throw std::runtime_error`) | CMake の `juiz_generate_types(<name> IDL idl/<file>.jidl DEPENDS <import した写し>)` がビルドのときに `build/juiz_idl/juiz_idl.h` を作る。juiz_std の型も一緒に作る | CMakeLists.txt(`cpp/idl/`)に `juiz_generate_types` を足す |
| Haskell | `src/<Name>.hs`(`error "…"`) | `scripts/dev.sh` がビルドの前に `juiz idl -g haskell -o gen idl/<file>.jidl` で `gen/` に作る(juiz_std のモジュールも)。`gen/` は .gitignore に入れる | .cabal(`haskell/idl/`)の `hs-source-dirs` に `gen`、`other-modules` に型のモジュール、`build-depends` に bytestring・containers・text |
| Zig | `src/<name>.zig`(`return error.NotImplemented`) | build.zig の `juiz.addTypes(.{ .files = idl/<file>.jidl, .depends = <import した写し> })` がビルドのときに作る(juiz_std の型も)。`juiz-idl` が PATH に無ければ `juiz idl` を使う(datatype.md §26.3) | build.zig(`zig/idl/`)に `addTypes` と、`addPlugin` の `.imports` に `juiz_types` |

- Rust: juiz_std の型(名前空間が `juiz_std` か `juiz_std.` で始まる型)が読んだ型にあれば、Cargo.toml に `juiz_std_types = "=<版>"` を足し、build.rs に `.extern_path("juiz_std", "::juiz_std_types::juiz_std")` を書く(datatype.md §24.2。同じ型が 2 つの Rust の型にならないように)。無ければどちらも書かない。`juiz_idl` と `juiz_std_types` の版は juiz_sdk と同じ(§7.1)。`--juiz-source` ではソースの `juiz_idl`・`juiz_std_types` を指す。ContainerProcess は `&mut ContainerImpl<…>` で受ける。rust-skeleton の Container の状態のコメントは英語にした。
- Python: .jidl を変えたら型を作り直す(`juiz idl -g python --extern juiz_std -o app/python idl/<file>.jidl`、README に書く)。ビルドの手順が無いので自動では作り直さない。
- C++: `juiz_generate_types` は `juiz-idl` が無ければ PATH の `juiz` の `juiz idl` を使う(#468 で `bindings/cppjuiz/cmake/juiz_idl.cmake` に足した。libjuiz_sdk のアーカイブに入っているもの)。ビルドのときに juiz が PATH に要る(README に書く)。
- Zig: 関数は juiz_idl の `zig_skeleton_items`(datatype.md §26.4)。要素は 1 行ずつで、`juiz pkg add` はその並びの後ろに足す(§2.4)。.jidl の import を増やしたら、build.zig の `.depends` に足す(README に書く)。
- Haskell: 型のモジュール名が Component のモジュール名 `<Name>`(§3.1)と同じになるとき(名前 `myrobot` と名前空間 `myrobot` など)はエラー。.jidl に名前空間が増えたら、.cabal の `other-modules` に足す(README に書く)。

### 6.6 Zig(#577)

```
build.zig
build.zig.zon
src/<name>.zig
```

- build.zig.zon: `.name = .<name>`、`.version = "0.1.0"`、`.minimum_zig_version = "0.17.0"`。依存は zigjuiz 1 つで、書き方は zigjuiz の場所を指定したかで変わる(#578)。
  - 既定(`--zigjuiz-dir` も `--juiz-source` も無い): `.dependencies` は空で、コメントに `zig fetch --save=zigjuiz <URL>` を書く。`<URL>` は Releases の zigjuiz のアーカイブ `https://github.com/OpenJUIZ/juiz/releases/download/v<版>/zigjuiz-<版>.tar.gz`(juiz_pkg の `zigjuiz_url`、CONTRIBUTING の「リリース」)。`.hash` はアーカイブの中身から zig fetch が計算するもので、雛形を作るときには分からないため、scripts/dev.sh が初めての実行のときに `zig fetch --save=zigjuiz <URL>` で `.url` と `.hash` を書き込む(build.zig.zon に `.zigjuiz =` の行が無いとき。ネットワークが要るのはこの 1 回だけ)。juiz のソースは要らない。
    - `.url` だけを書いて `.hash` を書かない形は採らない。`zig build` は `.hash` が無いとエラーにし、Zig 0.17.1 の `zig fetch --save` はその行を `.hash` を足さずに書き直してしまうため(2026-11-22 確認)。
  - `--zigjuiz-dir`・`--juiz-source` を付けたとき: `.zigjuiz = .{ .path = "<zigjuiz の場所>" }`。juiz のテスト(pkg_cli_test)と、まだ公開していない版で試すときに使う。
  - Zig は依存の `.path` に絶対パスを受けないので、`<zigjuiz の場所>` は**雛形のディレクトリからの相対パス**で書く(生成時に作り先と zigjuiz の場所から計算する。作り先の在る一番深い親はシンボリックリンクをたどってから比べる)。相対パスにできない(Windows でドライブが違う)ときはエラー。雛形か zigjuiz を動かしたら直す(README に書く)。
  - zigjuiz の場所は `--zigjuiz-dir`、省けば `--juiz-source` の `bindings/zigjuiz`。生成時にそこに `build.zig.zon` が無ければ警告し、用意するコマンド(`git clone --depth 1 --branch v<版> …`)を表示する。
  - `.fingerprint` は Zig 0.17 が必須にした値で、上位 32 bit が `.name` の CRC-32(IEEE)、下位 32 bit がパッケージごとの乱数(0 と 0xffffffff 以外)。生成時に作る。
  - `.name` は識別子で 32 文字まで(§3 の 8)。
- build.zig: `zigjuiz.init`(`--release` で ReleaseSafe)→ `juiz.addPlugin(.{ .name = "<name>", .root_source_file = "src/<name>.zig" })` → `juiz.installBundle(lib, .{})`。`installBundle`(zigjuiz、app-package.md §12.2)が `zig-out/plugins/` にプラグインと libjuiz_sdk を並べ、rpath を `$ORIGIN` / `@loader_path` だけにする。
- `src/<name>.zig`: `juiz.exportComponent` の `.parts` に `juiz.processPart`・`juiz.containerPart`・`juiz.containerProcessPart` を並べる(zigjuiz の README の Component)。関数は C++・Haskell と同じく `<name>_add`・`<name>_create`・`<name>_increment`・`<name>_get`、状態の型は `<Name>State`(Haskell と同じ。zigjuiz・Zig の名前と重ならないように)。引数は struct 1 つ(`args: struct { a: i64 = 0, b: i64 = 0 }`)、足し算は `std.math.add` で溢れを `error.Overflow`(呼び出しのエラー -1101)にする。`<name>_get` は状態を `*const` で受けて readonly にする。`#[cfg(test)]` に当たるテストは置かない(`zig build test` に libjuiz_sdk のリンクが要り、雛形が大きくなるため)。
- scripts/dev.sh: (build.zig.zon に zigjuiz が無ければ `zig fetch --save=zigjuiz <URL>`)→ `zig build [--release] [-Dtarget=…] [-Djuiz-sdk=…]` → `zig-out/plugins/` の中身(プラグインと libjuiz_sdk)を `app/plugins/<ターゲット>/` に写す。
  - zig は環境変数 `ZIG` で変えられる(`ZIG="python3 -m ziglang"`)。`zig version` が 0.17 でなければ止める(zigjuiz の build.zig もコンパイルエラーにする)。
  - `--target <ターゲット>`: 別の機械向けにクロスビルドする(app-package.md §12.3)。ターゲットは libjuiz_sdk を配っている 5 種(Linux・macOS の 4 種と `x86_64-pc-windows-msvc`)で、Zig の名前(`aarch64-linux-gnu`・`aarch64-macos`・`x86_64-windows` など)に直して `-Dtarget` に渡す。libjuiz_sdk は `JUIZ_SDK_DIR`、無ければ `juiz sdk path --target <ターゲット>`(`juiz sdk install --target`、#577 で足した)。ホストの分は `-Dtarget` を付けずにネイティブでビルドする。Windows 向けは Zig 既定の ABI(`x86_64-windows-gnu`)のまま libjuiz_sdk の `lib/juiz_sdk.dll.lib` にリンクし、`zig-out/plugins/` に `<name>.dll` と `juiz_sdk.dll` が並ぶ(zigjuiz の installBundle、#579)。
  - libjuiz_sdk の場所は `JUIZ_SDK_DIR`、無ければ `--juiz-sdk-dir` に指定した場所(dev.sh に書く。省けば行ごと無い)、それも無ければ zigjuiz が探す(`JUIZ_SDK_HOME`・`juiz sdk path`)。
- scripts/pack.sh: §5.4 と同じ(`--no-build` で無ければ `scripts/dev.sh --release`)。`app/plugins/` の下を全部入れるので、先に `--target` で作った別の機械の分も同じパッケージに入る(README に手順)。
- .gitignore は `zig/_base/.gitignore.tmpl` で `common/_base` のものを置き換える(`/zig-out`・`/.zig-cache` と、`/app/plugins/`・`/dist/`・`*.juizapp`)。
- scripts/dev.ps1・pack.ps1(#588): dev.sh・pack.sh と同じことを PowerShell で行う。オプションは `-Release`・`-Target <ターゲット>`(dev.ps1)と `-NoBuild`(pack.ps1)。`$env:ZIG` は引数付きのコマンド(`python -m ziglang`)でもよい。zig は進み具合を標準エラーに書くので、外部コマンドの標準エラーをエラーとして扱わない(Windows PowerShell 5.1 の `$ErrorActionPreference = "Stop"` で止まらないように)。libjuiz_sdk の DLL は Windows に rpath が無いので、プラグインの隣に置く(`zig-out/plugins/` をそのまま写す)。

確かめたこと(2026-11-22、#578): 既定の形で生成し、dev.sh の `<URL>` を手元の HTTP サーバーに置いた zigjuiz のアーカイブ(`scripts/package-zigjuiz.sh`)に向けて、dev.sh の zig fetch → ビルド → 起動 → call が通る。

確かめたこと(2026-11-23、#588): Linux x86_64 で `--target x86_64-pc-windows-msvc` と Releases の libjuiz_sdk 0.3.3 の Windows のアーカイブで、`app/plugins/x86_64-pc-windows-msvc/` に `<name>.dll`(`juiz_sdk.dll` を読み込む)と `juiz_sdk.dll` ができる。Windows の実機での dev.ps1 → 起動 → call → pack.ps1 → `juiz run` は pkg_cli_test(test-windows.yml)で確かめる。

確かめたこと(2026-11-22): Linux x86_64 で、生成 → `juiz pkg add` → dev.sh → 起動 → call → pack → `juiz run` が通る(pkg_cli_test)。`--target aarch64-unknown-linux-gnu`・`--target aarch64-apple-darwin` で Releases の libjuiz_sdk 0.3.2 にリンクしたプラグインができ、3 ターゲットの .juizapp を作れた(別の機械での読み込みは未確認)。

## 7. 版の埋め方

### 7.1 版

雛形に書く juiz の版 `<版>` は、`juiz pkg` を動かした juiz 自身の版、つまり juiz_pkg の `env!("CARGO_PKG_VERSION")` にする(ワークスペースで juiz と同じ版)。

| ファイル | 書き方 | 理由 |
| --- | --- | --- |
| Cargo.toml | `juiz_sdk = "=<版>"` | C ABI なので厳密には不要だが、雛形を作った juiz とそろえておくと、マクロの書き方とホストの機能が食い違わない(teleop_twist と同じ) |
| pyproject.toml | `pyjuiz==<版>` | pyjuiz は juiz と同じ版で出している |
| CMakeLists.txt | `JUIZ_VERSION "<版>"` と `find_package(... EXACT)` | libjuiz_sdk はホストと版が一致しないと読み込めない |
| cabal.project | hsjuiz・libjuiz_sdk のパスに `<版>` が入る | 同上 |
| build.zig.zon | コメントの zigjuiz の URL に `<版>` が入る(dev.sh の zig fetch も同じ URL、#578) | 同上。zigjuiz も自分の build.zig.zon の版で libjuiz_sdk を探す |
| app/juizapp.yaml | `juiz: ">=<版>"`(C++・Haskell・Zig は `"=<版>"`、§5.2) | app-package.md §4.3 の pack の既定と同じ。C++・Haskell・Zig は同梱する libjuiz_sdk の版がホストと一致しないと読めないため |
| README.md | 要る juiz の版 | |

### 7.2 `--juiz-source`(開発者向け)

juiz 自身のテストと CI では、まだ公開していない版で雛形をビルドしたい。`--juiz-source <juiz のリポジトリ>` を付けると、公開版の代わりにソースを使う。

| 言語 | 変わるところ |
| --- | --- |
| Rust | `juiz_sdk = { path = "<dir>/juiz_sdk" }` |
| Haskell | hsjuiz の場所が `<dir>/bindings/hsjuiz`(`--hsjuiz-dir` より弱い) |
| Zig | zigjuiz の場所が `<dir>/bindings/zigjuiz`(`--zigjuiz-dir` より弱い) |
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

- 言語ごとに先頭の手順が変わる: Python は `python3 -m venv .venv` → 有効化 → `pip install -e .`、C++ は SDK の用意(無ければ)→ `scripts/dev.sh`、Haskell は hsjuiz・SDK の用意(無ければ)→ `scripts/dev.sh`、Zig は zigjuiz(`--zigjuiz-dir`・`--juiz-source` を付けてそこに無ければ、juiz のソース)・SDK の用意(無ければ)→ `scripts/dev.sh`(既定の形では zigjuiz を dev.sh が zig fetch で取る)。
  - C++ の「無ければ」は、SDK の場所(`--juiz-sdk-dir`、省けば `~/.juiz/sdk/<版>`)に `lib/cmake/juiz_sdk/juiz_sdkConfig.cmake` が無いとき。用意するコマンドは `juiz sdk install`(juiz#548、OS によらず同じ。版は juiz と同じなので付けない)で、`--juiz-sdk-dir` を付けたときはその場所を `--dir` で付ける。README の手順(`next_steps`)には、在るかどうかによらず書く(Step 2)。
- Windows で動いているときは `scripts\dev.ps1` の形で表示する。
- 呼び出しの例は、そのテンプレートに入っている型の分だけ出す。
- 警告(§6.4 の hsjuiz が無いなど)は、この表示の前に出す。

## 10. テンプレートの置き場所と置換

### 10.1 置き場所

```
juiz_pkg/templates/
├─ common/_base/            どの言語・種類にも入れるもの(.gitignore の共通部分、LICENSE など)
├─ common/design/           設計文書から作るときの app/app.conf(-t・-d、中身は design_conf)
├─ common/idl/              --idl の app/app.conf
├─ <lang>/_base/            言語ごと・種類によらないもの(ビルドのファイル、scripts/、README)
├─ <lang>/<template>/       言語と種類ごとのもの(Component のソース。C++・Haskell・Zig だけ、設計文書 Step 3 まで)
├─ <lang>/idl/              --idl のもの(Rust の -d で .jidl を写すときも Cargo.toml・build.rs に使う)
└─ <lang>/design/           -d の README(Rust・Python)

juiz_pkg/templates/design/fragments/<lang>/<役割>.<拡張子>.tmpl  -t の本体の断片(design.md §7.2。create の層には入らない)

juiz_pkg/templates/add/<lang>/<process|container>.tmpl   juiz pkg add で足すコード(§2.4。create の層には入らない)
juiz_pkg/templates/design/<template>.juizdesign.yaml.tmpl  -t のプリセットの設計文書(design.md §7.1。create の層には入らない。変数は name・Name・juiz_version)
juiz_pkg/templates/add/zig/<process|container>_part.tmpl  Zig の juiz.exportComponent の .parts に足す要素(#577)
```

- `<lang>` は `rust` / `python` / `cpp` / `haskell` / `zig`、`<template>` は `component` / `process` / `container`。`--idl` のときは `<template>` の代わりに `idl` の層(`common/idl/`・`<lang>/idl/`)を重ね、その後に .jidl の写し・Python の型のコード・Component のソースをコードで足す(§6.5)。
- 雛形は `common/_base` → `common/<template>` → `<lang>/_base` → `<lang>/<template>` の順に重ね、同じ出力パスは後のものが勝つ。
  - 設計文書 Step 2(#595)から: `-t` は `common/_base` → `common/design` → `<lang>/_base` → `<lang>/<template>`(Rust・Python には無い)、`-d` は `common/_base` → `common/design` → `<lang>/_base` →(Rust で .jidl を写すとき `rust/idl`)→ `<lang>/design`。その後に設計文書・.jidl の写し・Python の型のコード・Component のソース(Rust・Python)をコードで足す。
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
| `plugin_language` | conf の `language`(`rust` / `python` / `c++` / `haskell` / `zig`) |
| `plugin_path` | conf の `path`(`plugins/{target}` / `python`) |
| `juiz_sdk_dep` | Rust の依存の右辺(`"=<版>"`、`--juiz-source` なら `{ path = "…" }`) |
| `juiz_sdk_dir` | C++・Haskell の libjuiz_sdk の場所(§6.3・§6.4) |
| `juiz_sdk_dir_arg` | `--juiz-sdk-dir` に指定した場所(絶対パス)。省けば無し(C++ の CMakeLists.txt §6.3、Zig の scripts/dev.sh §6.6) |
| `hsjuiz_dir` | Haskell の hsjuiz の場所(§6.4) |
| `zigjuiz_dir` | Zig の zigjuiz の場所(絶対パス、README 用、§6.6)。指定しなければ無し |
| `zigjuiz_path` | build.zig.zon に書く zigjuiz の場所(雛形からの相対パス、§6.6)。指定しなければ無し |
| `zigjuiz_url` | Releases の zigjuiz のアーカイブの URL(dev.sh の zig fetch、§6.6、#578) |
| `zigjuiz_fetch` | `zigjuiz_url` と同じ。zigjuiz の場所を指定したときは無し(build.zig.zon のコメントと README 用) |
| `zig_fingerprint` | build.zig.zon の `.fingerprint`(`0x` と 16 桁、§6.6) |
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
| `idl_depends` | C++ の `juiz_generate_types` の `DEPENDS`(import した .jidl の写し、空白区切り)。無ければ無し(#468) |
| `idl_hs_modules` | Haskell の .cabal の `other-modules` に足す型のモジュール(`\n                    , A.B` を並べたもの)。`--idl` でなければ無し(#468) |
| `idl_hs_gen_dir` | Haskell の `--idl` の型の出力先 `gen`。ほかは無し(scripts/dev.sh の `juiz idl` の行と .gitignore の `/gen/` を消す、#468) |
| `idl_compile` | Rust の build.rs の `.compile(&[…])` の中身(`"idl/a.jidl", …`)。`--idl` は `idl_main`、`-d` は写す `idl:` の .jidl。ほかは無し |
| `description_str` | Cargo.toml・pyproject.toml・`app/juizapp.yaml` の `description` の値(`"…"` で囲んだもの)。`component.description`、無ければ `"<name>: a juiz component"` |
| `design_conf` | 設計文書から作る app.conf の `processes`・`containers`(とプリセットの `ecs`)(複数行、design.md §6.3)。`--idl` では無し |
| `design_idl_note` | `-d` の Python の README の「.jidl を変えたら型を作り直す」段落。写す .jidl が無ければ無し |
| `idl_zig_depends` | Zig の build.zig の `addTypes` の `.depends`(import した .jidl の写しを `b.path("…")` で `, ` 区切り。無ければ空)。`--idl` でなければ無し(#577) |

`types_table` と `next_steps` は、種類・言語で変わる README の部分を、条件分岐の書式を設けずに 1 つのテンプレートで書くために足した(Step 1)。

conf の `{target}` は juiz の置換(app-package.md §5.2)で、この書式とは別。`{{` と `{` は重ならない。

## 11. テスト

各 Step で、次を通さなければならない。

- 名前の規則(§3)の合う例・合わない例と、§3.1 の変換。
- 置換(§10.2)の、知らない変数・残った `{{`・行ごと消す規則。全言語・全種類の組み合わせ(4 × 3)を生成して、`{{` が残らないこと。
- 作り先の規則(§8): ファイルが在る・空でないディレクトリ・空のディレクトリ・途中で失敗したときに何も残らないこと。
- 生成から配布まで(言語の Step ごと): 一時ディレクトリに `--juiz-source` で生成 → `scripts/dev.sh` → `juiz -f app/app.conf` で起動 → 入っている型を全部 call して §4 の値が返る → `scripts/pack.sh` → `juiz run` で起動できる。Rust・Python は Step 1、C++ は Step 2、Haskell は Step 3。CI では、その言語の道具が入っている OS で回す(Haskell は Linux・macOS)。
  - Step 2 の C++: ワークスペースの `cargo build` でできた libjuiz_sdk を `scripts/package-juiz-sdk.sh` で配布と同じアーカイブにして一時ディレクトリに展開し、`--juiz-sdk-dir` で指す(`find_package(… EXACT)` と `juiz_sdk_bundle` を配布物と同じ形で通すため)。ポートは 8077。.juizapp に libjuiz_sdk が入っていることと、`juiz: "=<版>"` になっていることも確かめる。cmake と C++ コンパイラが要る(scripts/test.sh と同じ)。
  - Step 1 の形(juiz_app/tests/pkg_cli_test.rs、今は juiz_app/tests/cli/pkg.rs): Unix で回す(`.sh` を使うため。`.ps1` は自動のテストが無い)。ほかのテストとポートが重ならないよう、生成した conf のポートを 8078(Rust)・8079(Python)に書き換えてから起動する。`scripts/dev.sh` でビルドした後なので、pack は `scripts/pack.sh --no-build` で release ビルドを省く。Python は `PYTHONPATH` の bindings/pyjuiz を使う(scripts/test.sh が設定する)。
- `--idl`(Step 4・#468): juiz_pkg の単体テストで、言語ごとに生成するファイルの一覧・置換が残らないこと・juiz_std を使う/使わないときの依存の書き方・import した .jidl の写し・エラー(`-t` と一緒、宣言なし、2 つの名前空間、ディレクトリの外の import、各言語のキーワード、Python で既定値のある引数の後ろに既定値のない record/enum の引数)。juiz_idl の単体テストで Python・C++・Haskell の skeleton 生成器の出力。pkg_cli_test では、4 言語それぞれ juiz_std の型と別ファイルの record を使う .jidl から作り、本体を書き換えてから dev.sh(Python は不要)→ 起動 → call → `juiz type list --long` に .jidl の型が出る → pack.sh → `juiz run` を通す(Unix、ポートは Rust 8082・Python 8083・C++ 8084・Haskell 8085。Haskell は ghc と cabal がないときは飛ばす)。
- `juiz pkg add`(Step 5): juiz_pkg の単体テストで、4 言語 × 3 種類の雛形すべてに Process と Container を足せて(`{{` が残らず、元の型も残る)、各言語の一覧の書き換え・conf のブロックの位置(無いときに作る位置を含む)・エラー(conf に既にある、ソースの識別子と衝突、雛形でない、入れる所が無い)・2 つめの書き込みで失敗したときに元に戻ること。pkg_cli_test の Rust・Python・C++・Haskell の生成から配布までのテストでは、生成の直後に `juiz pkg add process <name>_sum` と `juiz pkg add container <name>_arm` を打ち(同じ名前をもう一度足すとエラーで conf が変わらないことも確かめる)、起動と `juiz run` の後に足した型も呼ぶ。
- Zig(#577): juiz_pkg の単体テストで、生成するファイルの一覧・build.zig.zon の相対パスと fingerprint(CRC-32 が Zig の勧める値と同じ)・名前の規則(§3 の 8 と 32 文字)・Windows で警告せず `.ps1` を置くこと(#588)・`juiz pkg add` の `.parts` への足し方(`--idl` の雛形に std を足すことも)・`--idl` の build.zig。全言語・全種類の組み合わせの置換と `juiz pkg add` には Zig も入れる(5 × 3)。zigjuiz の場所を指定しない既定の形(build.zig.zon に依存が無く、dev.sh・README に zig fetch の URL がある、#578)も単体テストで確かめる。pkg_cli_test(slow)で、C++ と同じ配布と同じ形の libjuiz_sdk を `--juiz-sdk-dir` で指し、zigjuiz は `--juiz-source` の `bindings/zigjuiz` で、`-t component`(ポート 8081)と `--idl`(ポート 8088)の生成から配布までを通す。.juizapp に libjuiz_sdk が入っていることと `juiz: "=<版>"` を確かめる。Zig 0.17 が無ければ飛ばし(`ZIG` で zig を変えられる)、`JUIZ_REQUIRE_ZIG=1` なら落とす。Zig の 2 件は Windows でも走らせ(#588、test-windows.yml)、scripts は `.ps1` を PowerShell(`pwsh`、無ければ `powershell`)で実行する。Windows の libjuiz_sdk は package-juiz-sdk.sh を使わず、ワークスペースの `juiz_sdk.dll`・`juiz_sdk.dll.lib` を配布と同じ `bin/`・`lib/` に写したものを `--juiz-sdk-dir` で指す。Windows では雛形を TEMP ではなく `target/pkg-test-work/` に作る(build.zig.zon の zigjuiz は相対パスなので、TEMP の C: とチェックアウトの D:(CI)のようにドライブが違うと作れない)。クロスビルドはネットワーク(Releases の libjuiz_sdk)が要るので自動のテストにはせず、§6.6 の「確かめたこと」に手で確かめた結果を書く。
- 設計文書 Step 2(#595): design.md §12 の Step 2。`-t` の 5 言語 × 3 種類の生成物のスナップショット(`juiz_pkg/tests/snapshots/`、design.md §7.3)、`create -d` の単体テストと pkg_cli_test(Rust 8090・Python 8102)。
- 設計文書 Step 1(#594): juiz_pkg の単体テスト(src/design/)で、design.md §12 の Step 1 のもの。§5 の規則ごとの合わない例(道筋と行)、プリセット 3 つと §3.1 の例が 5 言語の検査を通ること、juiz が書いた文書の読み書きが同じ文字列になること、JSON Schema との食い違い(テストの中の小さな JSON Schema の検査器で、schema が使う言葉だけを解く)、5 言語 × 3 種類の `create -t` に `juizdesign.yaml` が置かれてその言語の検査を通ること。pkg_cli_test(`pkg_design_and_check`)で `juiz pkg design`・`design check` の表示・終了コード・上書きしないこと。
- Haskell: テンプレートの `bundle-plugin-deps.sh` が juiz の `scripts/bundle-plugin-deps.sh` と一致すること。
  - Step 3 の Haskell(pkg_cli_test): C++ と同じく配布と同じ形の libjuiz_sdk を `--juiz-sdk-dir` で指し、hsjuiz は `--juiz-source` の `bindings/hsjuiz`。ポートは 8076。.juizapp に libjuiz_sdk・hsjuiz・GHC の RTS が入っていることと `juiz: "=<版>"` を確かめる。ghc・cabal(Linux では patchelf も)が無ければ飛ばし、`JUIZ_REQUIRE_HASKELL=1` なら落とす。cargo test が LD_LIBRARY_PATH に足す `target/` の下は、scripts を呼ぶときに外す(残すと bundle-plugin-deps.sh の確かめで、隣に写した libjuiz_sdk より先に target/debug のものが見つかって失敗する)。

## 12. 後回しにするもの

| もの | 今の扱い | 後で足すなら |
| --- | --- | --- |
| `--idl` の Python・C++ の `T?`・`bytes` の引数 | エラー(§6.5、datatype.md §20.6・§21.6) | pyjuiz のデコレータ・cppjuiz の関数が受け取れるようにしてから |
| 外部のテンプレート(git・cargo-generate 互換) | しない | テンプレートの置き場所を指定するオプション |
| 基礎パッケージ(`kind: base`)・`depends` の雛形 | しない(`kind: app` だけ) | `--kind base` など |
| 複数クレート・複数 Component のワークスペース | しない(1 雛形 = 1 Component) | |
| Haskell の Windows | 生成するが警告 | #137 の後に `.ps1` と DLL の作り方を足す |
| Zig の zigjuiz の `.hash` | 雛形に書かず、dev.sh の初回の `zig fetch --save` に任せる(#578) | 版ごとのハッシュを juiz_pkg に埋め込む(公開のたびにアーカイブから計算して書き込む仕組みが要る) |
| CI のワークフロー(.github/workflows)の雛形 | 置かない | 置く場合は `${{ }}` と置換の書式の衝突を決める |

## 13. Step との対応

| Step | issue | この文書の節 |
| --- | --- | --- |
| 0 仕様書 | #456 | 全体 |
| 1 juiz_pkg クレート、`juiz pkg create`・`juiz pkg target`・`juiz-pkg`、Rust と Python | #457 | §2、§3、§4、§5、§6.1、§6.2、§7、§8、§9、§10、§11 |
| 2 C++ | #458 | §6.3 |
| 3 Haskell | #459 | §6.4 |
| 4 `--idl` | #460(Rust)、#468(Python・C++・Haskell) | §6.5 |
| 5 `juiz pkg add`(任意) | #461 | §2.4、§5.1 |
| 6 サイトとリリース | #462、OpenJUIZ.github.io#87 | — |
| Zig(Zig Step 5) | #577 | §6.6 ほか |
| Zig の zigjuiz を Releases から zig fetch で取る(Zig Step 6) | #578 | §2(`--zigjuiz-dir`)、§6.6、§7.1、§10.2、§12 |
| 設計文書(design.md の Step 0〜6) | #592(#593〜#599) | §1、§2.2、§2.4〜§2.7、§4、§5 |
