# アプリ配布形式(.juizapp)仕様

- 状態: 確定(.juizapp Step 1、issue #283、2026-10-30)
- 親 issue: #282。検討の経緯と比較はプロジェクトの計画書 app-package-plan.md と検討資料(2026-10-30 決定)にある
- 関連: IPC ブローカー [ipc-broker.md](ipc-broker.md)(基礎パッケージの IPC 名)、データ型 [datatype.md](datatype.md)(接続時の型の指紋)、Rust プラグインの C ABI [rust-plugin-abi.md](rust-plugin-abi.md)(§4.1・§4.3・§6.3・§9・§12 の Rust の扱い、#328 で直した)
- この文書は #284〜#288 の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と範囲

Process などのプラグインと conf をまとめた 1 つのファイルを配り、受け取った人が `juiz run` だけで動かせるようにする。

- 入れ物は zip、拡張子は `.juizapp`。ルートにマニフェスト `juizapp.yaml` を置く。
- 1 ファイルに複数の OS・アーキテクチャのバイナリを入れてよい。
- juiz 本体は入れない。受け取る側に juiz(cargo の `juiz` か pip の `juiz-cli`)が入っている前提にする。
- ロボットの基礎部分を「基礎パッケージ」(`kind: base`)として常駐させ、別のアプリ(`kind: app`)が別プロセスからそれにつなぐ形を扱う。

扱わないもの(§13 で後回しにしたもの)は、署名、juiz を同梱した単体版、conf の include による同一プロセス方式、基礎の自動起動、ネーミングサービス。

### 1.1 用語

| 用語 | 意味 |
| --- | --- |
| パッケージ | `.juizapp` のファイル 1 つ。中身は §3 の構造の zip |
| マニフェスト | パッケージのルートの `juizapp.yaml`(§4) |
| アプリ | `kind: app` のパッケージ。基礎に依存してよい |
| 基礎(基礎パッケージ) | `kind: base` のパッケージ。常駐し、アプリから subsystem としてつながれる |
| ターゲット | OS とアーキテクチャの組。Rust の target triple で表す(§5) |
| ホスト | パッケージを動かそうとしている juiz。そのターゲットを「ホストのターゲット」と呼ぶ |
| 展開先 | `juiz run` がパッケージを展開したディレクトリ(§7.2)か、`juiz install` が置いたディレクトリ(§8.1) |

## 2. 全体の流れ

```
作る人                                   使う人
  app/ (juizapp.yaml・conf・plugins/…)
     │ juiz pack app/
     ▼
  talker_listener-0.1.0.juizapp ──配る──▶ juiz run talker_listener-0.1.0.juizapp
                                           │ 検証(§9)→ キャッシュに展開(§7.2)→ entry の conf で起動
                                           ▼
                                         juiz(使う人が入れたもの)

  robot_base-1.2.0.juizapp ──────────────▶ juiz install robot_base-1.2.0.juizapp
                                           juiz run robot_base          ← 基礎を常駐(ipc://robot_base)
                                           juiz run my_patrol-0.1.0.juizapp
                                                                        ← depends を見て ipc://robot_base を subsystem に
```

## 3. ディレクトリ構造

zip のルートからの相対パスで次のように置く。`juizapp.yaml` 以外の名前は慣例で、conf から参照できればどこに置いてもよい。

```
talker_listener-0.1.0.juizapp   (zip)
├─ juizapp.yaml                 マニフェスト(必須、ルート固定)
├─ app.conf                     起動する conf(既定の entry)。複数置いてよい
├─ plugins/
│  ├─ x86_64-unknown-linux-gnu/  ターゲットごとのネイティブプラグイン
│  │  ├─ libtalker.so
│  │  └─ libjuiz_sdk.so         C++ / Haskell のプラグインがあるとき(§12)
│  ├─ aarch64-apple-darwin/
│  │  └─ libtalker.dylib
│  └─ x86_64-pc-windows-msvc/
│     └─ talker.dll
├─ python/                      OS に依存しない .py
│  └─ listener.py
├─ types/                       IDL(.jidl)と生成した型(任意)
├─ assets/                      画像・モデルなど(任意)
└─ LICENSE                      (任意)
```

- ネイティブのプラグイン(Rust・C++・Haskell と、ブローカー・EC のプラグイン)は `plugins/<ターゲット>/` に置かなければならない。conf からは `path: "plugins/{target}"` で参照する(§5.2)。
- OS に依存しないもの(Python・型・資産)はターゲットで分けない。
- 予約するパス: `juizapp.yaml`、`juizapp.yaml.sig`(署名用、§13)、`bin/`(juiz 同梱の単体版用、§13)。format 1 のパッケージは `juizapp.yaml.sig` と `bin/` を含んではならない(pack も run もエラーにする)。

## 4. マニフェスト juizapp.yaml

conf と同じく YAML で書く。作る人が書く項目と、`juiz pack` が書き足す項目がある。

```yaml
format: 1
name: talker_listener
version: 0.1.0
kind: app
description: "talker と listener を Push でつなぐ例"
license: MIT OR Apache-2.0
juiz: ">=0.0.30"
entry: app.conf
depends:
  robot_base: ">=1.2, <2"
python:
  requires: ["numpy>=1.26"]
exclude: ["*.log", "scratch/**"]
# ここから下は juiz pack が書く
languages: [rust, python]
targets:
  - x86_64-unknown-linux-gnu
  - aarch64-apple-darwin
  - x86_64-pc-windows-msvc
rust:
  juiz_sdk: "0.0.30"
  abi: 1
files:
  app.conf: "sha256:3f5c…"
  plugins/x86_64-unknown-linux-gnu/libtalker.so: "sha256:9a01…"
  python/listener.py: "sha256:c2d7…"
```

### 4.1 項目

| 項目 | 型 | 必須 | 既定値 | 書く人 | 意味 |
| --- | --- | --- | --- | --- | --- |
| `format` | 整数 | ○ | — | 作る人 | パッケージ形式の版。この文書は `1` |
| `name` | 文字列 | ○ | — | 作る人 | パッケージ名(§4.2) |
| `version` | 文字列 | ○ | — | 作る人 | パッケージの版。semver 2.0(`1.2.0`、`0.1.0-rc.1`) |
| `kind` | `app` / `base` | | `app` | 作る人 | §10 |
| `description` | 文字列 | | なし | 作る人 | 一行の説明 |
| `license` | 文字列 | | なし | 作る人 | SPDX の式 |
| `authors` | 文字列の列 | | なし | 作る人 | |
| `homepage` | 文字列 | | なし | 作る人 | URL |
| `juiz` | 版の範囲 | ○(パッケージ内) | §4.3 | 作る人(省けば pack) | 動かせる juiz の版 |
| `entry` | パス | | `app.conf` | 作る人 | `juiz run` が使う conf。パッケージ内の相対パス |
| `depends` | 名前 → 版の範囲 | | なし | 作る人 | 依存する基礎パッケージ(§10.2) |
| `python.requires` | 文字列の列 | | なし | 作る人 | 要る Python のパッケージ。PEP 508 の要求(`numpy>=1.26`) |
| `python.version` | 文字列 | | なし | 作る人 | 要る Python の版。PEP 440 の指定(`>=3.10`) |
| `base.http_port` | 整数 | | なし | 作る人 | `kind: base` のみ。基礎の HTTP ブローカーの固定ポート(§10.1) |
| `exclude` | glob の列 | | なし | 作る人 | `juiz pack` で入れないファイル(§6.2)。パッケージ内では意味を持たない |
| `languages` | 列 | ○(パッケージ内) | — | pack | `rust` / `python` / `c++` / `haskell` のうち使っているもの |
| `targets` | 列 | ○(パッケージ内) | — | pack | 入っているターゲット(§5)。ネイティブのプラグインが無ければ `[any]` |
| `rust.juiz_sdk` | 文字列 | Rust を含むとき | — | pack | Rust のプラグインをビルドした juiz_sdk の版 |
| `rust.abi` | 整数 | C ABI の Rust のプラグインを含むとき | — | pack | Rust のプラグインの C ABI の版(rust-plugin-abi.md §9) |
| `rust.rustc` | 文字列 | Rust ABI の Rust のプラグインを含むとき | — | pack | Rust ABI のプラグイン(EC・ブローカーと、C ABI の入口が無い古いプラグイン)をビルドした `rustc -V` の出力(§12.1) |
| `files` | パス → `sha256:<小文字 16 進 64 桁>` | ○(パッケージ内) | — | pack | `juizapp.yaml` を除く全ファイルの SHA-256(§9.1) |

- 「○(パッケージ内)」は、pack 前の `juizapp.yaml` では省いてよく、`.juizapp` の中では必須という意味。
- 知らない項目は警告を出して無視する(同じ format の中で項目を足せるようにするため)。項目の意味を変えるとき、または古い juiz が無視すると危ない項目を足すときは `format` を上げる。
- 項目の値が型に合わなければエラーにする。

### 4.2 パッケージ名

- `[a-z0-9_-]` からなる 1〜64 文字で、英小文字で始まらなければならない。
- 基礎パッケージの名前はそのまま IPC ブローカーの名前になる(§10.1)。この規則は ipc-broker.md §2.1 の名前の規則に収まるように決めてある。
- 名前は大文字小文字を区別しない OS でも衝突しないよう、小文字だけにした。

### 4.3 版の範囲(`juiz` と `depends`)

- 書き方は Cargo の版の要求と同じ(`=0.0.28`、`>=1.2, <2`、`^1.2`、`~0.0.28`)。比較は semver で行う。
- `juiz` を省いたとき、pack は `>=<pack した juiz の版>` を書く。
- Rust ABI のプラグイン(§12.1)を含むとき、`juiz` は `=x.y.z` の形でなければならず、`x.y.z` は `rust.juiz_sdk` と一致しなければならない(pack が強制する、§6.3)。Rust ABI のプラグインは juiz_sdk の版がホストと違うと読み込めないため(`RustPlugin::check_build_info`)。
- Rust のプラグインが C ABI のものだけなら、この強制はしない。`juiz` の既定は上のとおり `>=<pack した juiz の版>`(rust-plugin-abi.md、#291 の決めごと B)。

## 5. ターゲット

### 5.1 ターゲット名

ターゲット名は Rust の target triple をそのまま使う。libjuiz_sdk のアーカイブ(sdk-release.yml)と同じ呼び名で、少なくとも次を扱う。

| ターゲット | OS / アーキ |
| --- | --- |
| `x86_64-unknown-linux-gnu` | Linux x86_64 |
| `aarch64-unknown-linux-gnu` | Linux arm64(Raspberry Pi 64bit、Jetson など) |
| `x86_64-apple-darwin` | macOS Intel |
| `aarch64-apple-darwin` | macOS Apple Silicon |
| `x86_64-pc-windows-msvc` | Windows x86_64 |

- 表に無い triple も書いてよい(pack は警告だけ出す)。
- `any` は「ネイティブのプラグインを含まず、どのターゲットでも動く」を表す。`any` は他のターゲットと並べてはならない。
- ホストのターゲットは、juiz をビルドしたときの target triple とする(juiz_core の build.rs で `TARGET` を埋め込む、#284)。

### 5.2 conf の `{target}` 置換(#284)

conf の `plugins` の下の各プラグインの `path` に書いた `{target}` を、ホストのターゲット名に置き換える。

```yaml
plugins:
  process_factories:
    talker:
      path: "plugins/{target}"     # Linux x86_64 では plugins/x86_64-unknown-linux-gnu
```

- 対象は `plugins` の下のすべての種類(`process_factories`、`container_factories` とその `processes`、`container_process_factories`、`components`、`broker_factories`、`ec_factories`)の `path`。
- 置き換えるのは文字列 `{target}` だけ。それ以外の `{…}` はそのまま残す。
- パッケージの外(普通の `juiz -f app.conf`)でも同じように置き換える。開発中の conf と配布する conf を同じにできる。
- 置き換えは conf を読んだ直後、相対パスの解決の前に行う(`System::new` で、`~/.juiz/conf/default.conf` を重ねた後)。実装は juiz_core の `replace_target_in_plugin_paths`、置き換え先は `juiz_core::HOST_TARGET`。

### 5.3 ブローカー・EC のプラグインのパス(#284)

0.0.27 までのブローカーのプラグイン(`plugins.broker_factories`)と EC のプラグイン(`plugins.ec_factories`)の `path` はカレントディレクトリからの相対で解決されていた(`system_builder/brokers.rs`・`ecs.rs` の `working_dir = None`)。他のプラグインと同じく **conf の親ディレクトリからの相対** に揃える。

- 絶対パスはそのまま使う。
- これは今の挙動を変えるので、#284 の PR と版上げの変更点に書く。

### 5.4 HTTP ブローカーの静的ファイルのパス

`option.http_broker.static_filepaths`(と `brokers` に書いた `type_name: http` のブローカーの `static_filepaths`)の相対パスは、0.0.29 まではカレントディレクトリからの相対で解決されていた。`juiz run` はカレントディレクトリを変えない(§7.1)ので、パッケージに入れた画面を配信できなかった。プラグインと同じく **conf の親ディレクトリからの相対** に揃える。

- 絶対パスはそのまま使う。conf を使わずに `System` を作ったとき(親ディレクトリが無いとき)は今までどおり。
- 古い conf のため、conf からの相対で見つからず、カレントディレクトリからの相対で見つかるときは、警告を出してカレントディレクトリからのパスを使う。
- 実装は juiz_core の `system_builder::http_broker::resolve_static_filepaths`。

## 6. `juiz pack`(#285)

```
juiz pack <dir> [-o <出力先>] [--target <triple>]...
```

`<dir>` の `juizapp.yaml` を読み、§6.2 のファイルを集め、マニフェストを書き足して zip にする。

### 6.1 出力

- 既定の出力ファイル名は `<name>-<version>.juizapp`。`--target` を 1 つだけ指定したときは `<name>-<version>-<target>.juizapp`。
- `-o` にディレクトリを渡せばその中に、ファイル名を渡せばその名前で書く。
- 出力先に同じ名前のファイルがあれば上書きする。

### 6.2 入れるファイル

`<dir>` の下の全ファイルから、次を除いたもの。

- 名前が `.` で始まるファイルとディレクトリ(`.git`、`.venv` など)
- `target/`、`__pycache__/`、`*.pyc`、`dist-newstyle/`、`build/`
- 出力する `.juizapp` 自身と、他の `*.juizapp`
- マニフェストの `exclude` の glob(`<dir>` からの相対パスに当てる。`**` は複数階層)
- `--target` を指定したときは、指定に無い `plugins/<ターゲット>/`

シンボリックリンクは辿らず、エラーにする(zip に入れられないため)。ファイル名は §11 の規則を満たさなければならない。

### 6.3 pack の検査と書き足し

pack は次を順に行い、どれかが満たされなければエラーにして何も書かない。

1. `format` が `1` で、§4.1 の作る人の項目が型に合う。`name` が §4.2、`version` が semver に合う。
2. `entry` の conf があり、YAML として読める。
3. entry の conf の `plugins` を読み、各プラグインについて:
   - `path` は相対パスで、`{target}` を置き換えた結果がパッケージの中を指していなければならない(`..` で外に出てはならない)。
   - ネイティブのプラグイン(`language` が `rust`(省略時)・`c++`・`haskell` のもの、と `broker_factories`・`ec_factories`)は `path` に `{target}` を含まなければならない。
   - 集めた各ターゲットのディレクトリに、そのプラグインのファイル(`lib<name>.so` / `lib<name>.dylib` / `<name>.dll`)がなければならない。
   - Python のプラグインは `<path>/<name>.py` がなければならない。
4. `languages` を 3 から求めて書く。作る人が書いてあれば、求めたものと一致しなければならない。
5. `targets` を `plugins/` の下のディレクトリ名から求めて書く(`--target` があればその列)。ネイティブのプラグインが 1 つも無ければ `[any]`。作る人が書いてあれば、求めたものと一致しなければならない。
6. Rust を含むとき、各 Rust のプラグインを C ABI か Rust ABI かに分ける(§12.1)。どちらもファイルのバイト列から文字列を探して読む(読み込まずに読むので、別ターゲットのバイナリでも読める)。
   - `broker_factories`・`ec_factories` のプラグインは Rust ABI。
   - conf のプラグインの項目に `abi: rust`(rust-plugin-abi.md §11)を書いたものも Rust ABI(ホストが Rust ABI で読むため)。`abi` が `c`・`rust` 以外ならエラー。
   - それ以外は、印 `juiz_rust_abi=<版>;`(rust-plugin-abi.md §9.3)があれば C ABI、無ければ Rust ABI(この仕組みより前の juiz_sdk でビルドしたもの)。
   - どのプラグインからも、ビルド情報の文字列(`juiz_sdk=…;rustc=…`、`juiz_sdk::build_info`)を読む。`juiz_sdk` の版はすべてのプラグイン・ターゲットで同じでなければならない。違えばどのファイルが違うかを示してエラー。`rust.juiz_sdk` に書く。
   - C ABI のプラグインがあれば、その版を `rust.abi` に書く。版はすべて同じでなければならない(違えばエラー)。
   - Rust ABI のプラグインがあれば、それらの `rustc` はすべて同じでなければならず、`rust.rustc` に書く。`juiz` を省いていれば `=<juiz_sdk>` を書く。書いてあれば `=<juiz_sdk>` と一致しなければならない(§4.3)。
   - Rust ABI のプラグインが無ければ `rust.rustc` は書かない。`juiz` を省いていれば `>=<pack した juiz の版>` を書く(§4.3)。書いてあり、その範囲が C ABI を読めない juiz(C ABI が入った 0.0.30 より前。pack は 0.0.29 が範囲に入るかで見る)を含むときは警告を出す。
7. `kind: base` で `base.http_port` があるとき、entry の conf に `option.http_broker.port` があれば同じ値でなければならない。`kind: app` で `base` を書いてはならない。
8. `depends` の名前が §4.2 に合い、版の範囲が読める。自分自身の名前を含んではならない。
9. 各ファイルの SHA-256 を `files` に書く(パスの順に並べる)。
10. §6.4 のとおり zip を書く。

C++ / Haskell のプラグインの隣に libjuiz_sdk が無いときは警告を出す(規則は #287 で決める、§12)。

### 6.4 zip の書き方

- 先頭のエントリを `juizapp.yaml` にし、残りはパスの辞書順に並べる。
- 圧縮は deflate か無圧縮だけを使う。
- 時刻は全エントリ 1980-01-01 00:00:00 にする(同じ入力から同じバイト列ができるように)。
- Unix のモードは、元のファイルに実行ビットがあれば 0755、なければ 0644 を書く。
- ディレクトリのエントリは書かない。
- 暗号化・分割・ZIP64 以外の拡張は使わない(4 GiB を超えるときだけ ZIP64)。

## 7. `juiz run`(#285)

```
juiz run <パッケージ> [--entry <conf>] [--depend <名前>=<URL>]... [その他の juiz のオプション]
```

`<パッケージ>` は次のどちらか。

- `.juizapp` のファイルのパス。名前が `.juizapp` で終わるか、そのパスにファイルがあればこちら。
- インストール済みのパッケージの名前(`robot_base`、`robot_base@1.2.0`)。版を省けば、インストール済みの中で一番新しい版を使う(§8)。

### 7.1 動作

1. §9 の順に検証する。
2. ファイルなら §7.2 のキャッシュに展開する。インストール済みならその置き場所をそのまま使う。
3. 展開先の `entry`(`--entry` があればそれ)の conf を読み、普通の `juiz -f` と同じく conf の親ディレクトリを基準にパスを解決する。
4. `kind: base` なら §10.1、`depends` があれば §10.2 の設定を conf に足す。
5. 常駐して動かす(`-d` と同じく、止めるまで動き続ける)。ただし HTTP ブローカーは `-d` のように強制で立てず、conf の `option.http_broker` と `--http`・`--no-http` に従う(下の表、#306。0.0.29 までの `-b` は `--http` に変わった)。

| conf の `option.http_broker` | 指定なし | `--no-http` | `--http` |
|---|---|---|---|
| 書いていない、または `start` が無い | 立つ(juiz の既定) | 立たない | 立つ |
| `start: true` | 立つ | 立たない | 立つ |
| `start: false` | 立たない | 立たない | 立つ |

- `--http` と `--no-http` を両方付けたら `--http` に従う(立つ)。
- `kind: base` の `base.http_port`(§10.1)は conf の `start: true` と同じ扱いで、`--no-http` なら立たない。
- http で基礎につなぐとき(§10.2)は HTTP ブローカーが要るので、`--no-http` を付けるとつなぐ前にエラーにする。
- HTTP を外に出したくない配布アプリは、conf に `start: false` を書いておく。書かないと 8000 番(`port` の既定)が開く。

- `--namespace`・`--ipc`・`--http`・`--no-http` など、juiz の他のオプションは普通の起動と同じ意味で効く(`kind: base` の `--ipc` は §10.1)。
  `juiz run` 自身が受け付けるのは `--entry`・`--http`・`--no-http`・`--ipc`・`--namespace`・`--depend`(`--http`・`--no-http`・`--ipc`・`--namespace` は `juiz --http run …` のように run の前に書いてもよい)。
- `--depend`、インストール済みの名前での起動(§8)、`kind: base` と `depends` の扱い(§10)は #286 で入れた。
- カレントディレクトリは変えない。環境変数 `JUIZ_APP_DIR` に展開先の絶対パスを入れて起動する(Python のプラグインが `assets/` などを開くとき用)。
- 展開先は読み取り専用として扱う。プラグインはそこに書き込まないこと(キャッシュは消されうる、同じ展開先を複数の起動が共有する)。

### 7.2 キャッシュ

ネイティブのプラグインは zip の中から直接は読めない(dlopen / LoadLibrary は実ファイルを要る)ので、展開してから起動する。

- 展開先は `<キャッシュ>/apps/<name>-<version>-<h>/`。`<h>` は `.juizapp` ファイル全体の SHA-256 の先頭 16 桁(小文字 16 進)。
- `<キャッシュ>` は次の順で最初に決まったもの。
  1. 環境変数 `JUIZ_CACHE_DIR`
  2. Linux / macOS: `$XDG_CACHE_HOME/juiz`、無ければ `~/.cache/juiz`
  3. Windows: `%LOCALAPPDATA%\juiz\cache`
- 展開先がすでにあり、その中に完了の印 `.juizapp-complete` があれば、展開を省いてそのまま使う(ファイル全体のハッシュが同じなので中身も同じ)。
- 展開は同じ親の一時ディレクトリ(`.tmp-<乱数>`)に行い、全部書いて `files` と照合してから `.juizapp-complete` を作り、展開先の名前に rename する。rename が「すでにある」で失敗したら(別の起動が先に終えた)、一時ディレクトリを消して既存のものを使う。
- 展開は juiz 自身が行うので、macOS の quarantine 属性は展開したファイルに付かない。
- キャッシュの掃除は format 1 では行わない(消してよい。次の `juiz run` が展開し直す)。

## 8. `juiz install`(#286)

```
juiz install <file.juizapp>
juiz uninstall <name>[@<version>]
juiz packages
```

### 8.1 置き場所

- `<データ>/packages/<name>/<version>/` に展開して置く。`juizapp.yaml` もそのまま置く。
- `<データ>` は次の順で最初に決まったもの。
  1. 環境変数 `JUIZ_DATA_DIR`
  2. Linux / macOS: `$XDG_DATA_HOME/juiz`、無ければ `~/.local/share/juiz`
  3. Windows: `%LOCALAPPDATA%\juiz\data`

### 8.2 動作

- `juiz install` は §9 の 1〜5(ファイルの検証まで)を行ってから、§7.2 と同じ手順(一時ディレクトリに展開 → 照合 → rename)で置く。ターゲットが合わないパッケージは入れない。
- 同じ名前・同じ版がすでにあれば、`files` が同じなら何もせず成功、違えばエラー(`robot_base 1.2.0 is already installed with different contents. Run 'juiz uninstall robot_base@1.2.0' first.`)。
- 同じ名前の別の版は並べて置ける。
- `kind: app` も `kind: base` も入れられる。依存の解決に使うのは `kind: base` だけ。
- `juiz install` は依存先を入れない(取ってくる場所が無いため)。足りない依存は警告を出す。
- `juiz uninstall <name>` は全版を、`<name>@<version>` はその版を消す。
- `juiz packages` はインストール済みのパッケージを `name version kind` の表で出す(見出しは `NAME VERSION KIND`、名前・版の順)。
- 置いた印 `.juizapp-complete` の無いディレクトリ(途中で止まった install)は、インストール済みとして扱わない。次の `juiz install` が消して置き直す。

## 9. 検証の順番

`juiz run` と `juiz install` は次の順で検証する。前の段で失敗したら、後の段は行わない。軽く、利用者が直しやすい原因から先に見る。

| 段 | 見ること | 失敗したとき |
| --- | --- | --- |
| 1. 入れ物 | zip として開ける。ルートに `juizapp.yaml` がある(1 MiB 以下) | `<file> is not a juiz app package (no juizapp.yaml at the root)` |
| 2. format | `format` を知っている(今は `1` だけ) | `<file> uses package format <n>, but this juiz (<版>) supports up to 1. Update juiz.` |
| 3. juiz の版 | ホストの juiz の版が `juiz` の範囲に入る | `<name> <ver> requires juiz <範囲>, but this is juiz <版>.` に続けて、`=x.y.z` なら `Install it with: cargo install juiz_app --version x.y.z` か `pip install juiz-cli==x.y.z` |
| 4. ターゲット | ホストのターゲットが `targets` にあるか、`targets` が `[any]` | `<name> <ver> is built for <targets の並び>, but this machine is <ホスト>.` |
| 5. ファイル | §11 の zip の安全規則と、`files` との照合(§9.1) | 規則・ファイルごとの文(§11、§9.1) |
| 6. ビルド情報 | `rust.abi` があれば、ホストがその C ABI の版を読める。`rust.rustc` があれば、ホストの rustc と一致する(§12.1) | `<name> <ver> contains Rust plugins with plugin ABI <n>, but this juiz supports ABI <版の並び>. Update juiz.` / `<name> <ver> contains Rust plugins built with <rustc>, but this juiz was built with <rustc>.` |
| 7. 言語 | `python` を含むなら、ホストが Python を埋め込める(embed-python)。`python.version` に合う | `<name> needs Python, but this juiz was built without Python support. Install juiz with: pip install juiz-cli` など |
| 8. Python の依存 | `python.requires` の各要求を、ホストが埋め込む Python の `importlib.metadata` で満たせる | 足りないものを並べ、`<その Python の実行ファイル> -m pip install "numpy>=1.26" …` を案内 |
| 9. 依存 | §10.2 | §10.3 |
| 10. 読み込み | 今までどおり、プラグインを読むときの検査(Rust の C ABI は表の ABI の版(rust-plugin-abi.md §9.2)、Rust ABI は `check_build_info`、C++ / Haskell は `juiz_sdk_build_info`)と、接続時の型の指紋(datatype.md §8・§19) | 既存のエラー |

- `juiz install` は 1〜6 まで行う(7〜9 は動かす機械で決まるので `juiz run` のときに見る)。
- インストール済みのパッケージを `juiz run` するときは、1・5 を省き(置いたときに済んでいる)、2〜4・6〜10 を行う。
- キャッシュに展開済み(`.juizapp-complete` あり)のときは、展開と照合を省く。zip の中の `juizapp.yaml` を読む 1〜4 と 6〜10 は毎回行う。

### 9.1 `files` との照合

- zip のエントリ(ディレクトリを除く、`juizapp.yaml` を除く)の集合と、`files` のキーの集合が一致しなければならない。
  - `files` に無いエントリがある: `unexpected file in package: <path>`
  - `files` にあってエントリが無い: `missing file in package: <path>`
- 各エントリを展開しながら SHA-256 を計算し、`files` の値と比べる。違えば `hash mismatch: <path>`。
- 照合に失敗したら展開途中の一時ディレクトリを消す。

## 10. 基礎パッケージと依存

基礎(`kind: base`)は常駐させ、アプリは別プロセスから subsystem としてつなぐ。データ型の指紋で接続の契約を確かめられるので、基礎とアプリは juiz の版も言語も独立にできる。

### 10.1 基礎を起動するとき(#286)

`kind: base` のパッケージを `juiz run` すると、次を conf に足す。

- IPC ブローカー `{type_name: ipc, name: <パッケージ名>}`(`--ipc <パッケージ名>` と同じ)。IPC ブローカーの名前は一意なので、同じ機械で同じ基礎は 1 つしか動かない(2 つ目は ipc-broker.md §2.5 の `already in use` で止まる)。
  - `--ipc` で別の名前を指定したらエラーにする(`a base package always uses its package name as the IPC name`)。
  - conf の `brokers` にすでに同じ名前の ipc ブローカーがあれば、足さない。
- `base.http_port` があれば、`option.http_broker.start: true` と `port: <http_port>` にする。`--http` を付けなくても HTTP ブローカーが立つ。
- system の profile(`system/profile_full`)に次を載せる。アプリはこれで相手を確かめる。

```json
"package": { "name": "robot_base", "version": "1.2.0", "kind": "base" }
```

`kind: app` を `juiz run` したときも `package` を載せてよい(管理画面での表示用)。パッケージを使わずに起動したときは載せない。

### 10.2 アプリを起動するとき(#286)

`depends` の各 `<名前>: <範囲>` について、つなぎ先を次の順で決め、subsystem として conf に足す。

1. コマンドラインの `--depend <名前>=<URL>`(`ipc://robot_base2`、`http://192.168.0.10:8100`、`http://192.168.0.10`)
2. アプリの conf の `subsystems` のうち、`package: <名前>` を持つもの
3. 既定: `{type_name: ipc, name: <名前>}`(`ipc://<名前>`)

```yaml
# アプリの conf で別の機械の基礎を使う例
subsystems:
  - package: robot_base       # この subsystem を depends の robot_base として扱う
    type_name: http
    name: "192.168.0.10"      # ポートを省くと、インストール済みの robot_base の base.http_port
```

- `type_name: http` でポートを省いたときは、インストール済みの基礎(範囲に合う一番新しい版)の `base.http_port` を使う。基礎が入っていないか `base.http_port` が無ければエラー(`cannot decide the port for robot_base. Write the port in the conf or --depend.`)。
- `package` を持たない `subsystems` は今までどおり扱い、依存の確認はしない。
- つないだ後、相手の profile の `package` を見て、`name` が一致し、`version` が範囲に入ることを確かめる(§10.3)。
- subsystem としてつなぐと、相手は同じ種類のブローカーでこちらへつなぎ返す(mastersystem)。そのため `juiz run` は、アプリ側にもブローカーを用意する。
  - ipc でつなぐとき、conf に ipc ブローカーが無く `--ipc` も無ければ、`<アプリ名>-<プロセス ID>` の名前で ipc ブローカーを足す(同じアプリを複数動かせるように)。
  - http でつなぐときは、`--http` が無くても HTTP ブローカーを立てる(ポートは conf の `option.http_broker.port`、既定 8000)。`--no-http` とは両立しないのでエラーにする(§7.1)。
- 確認(つないで profile を見る)は、アプリのブローカーを立てる前に行う。確認に失敗したときにソケットなどを残さないため。juiz_core の `System::set_subsystem_check` で、conf の `subsystems` の各要素について呼ぶ。
- 基礎とアプリの名前の衝突を避けるには、ネームスペースを分ける(基礎 `/robot1/base`、アプリ `/robot1/patrol` など)。juiz run はネームスペースを自動では付けない。

### 10.3 依存のエラー

つなぎ先に接続できない、または確認に失敗したときは、起動を止めて次を出す。

| 状況 | 文 |
| --- | --- |
| つなげない、基礎がインストールされていない | `my_patrol depends on robot_base >=1.2, <2, but robot_base is not installed. Install it with: juiz install robot_base-<version>.juizapp` |
| つなげない、基礎はインストールされている | `my_patrol depends on robot_base >=1.2, <2, but robot_base is not running at ipc://robot_base. Start it first with: juiz run robot_base` |
| つながったが `package` が無い | `ipc://robot_base is running but is not started from a package. Start it with: juiz run robot_base` |
| `package.name` が違う | `ipc://robot_base is <name>, not robot_base.` |
| 版が範囲外 | `my_patrol needs robot_base >=1.2, <2, but robot_base 2.0.1 is running at ipc://robot_base.` |

- 「インストールされているか」は `--depend` や conf で別の機械を指していても見る(案内の文を変えるためだけ)。
- 基礎を自動では起動しない(§13)。

## 11. zip の安全規則

展開(`juiz run`・`juiz install`)するときに次を拒否する。どれか 1 つでもあれば、何も残さずにエラーにする(`unsafe entry in package: <path> (<理由>)`)。

- 絶対パス(`/` で始まる、`C:` などのドライブ名、`\\` で始まる)
- `..` を含む、または空の要素・`.` の要素を含むパス
- `\` を含むパス(区切りは `/` だけ)
- 制御文字、`:`・`*`・`?`・`"`・`<`・`>`・`|` を含む名前、末尾が `.` か空白の要素
- Windows の予約名(`CON`、`PRN`、`AUX`、`NUL`、`COM1`〜`COM9`、`LPT1`〜`LPT9`、拡張子付きも)の要素
- UTF-8 として読めない名前
- 大文字小文字を同一視すると重なるパス(`A.so` と `a.so`)、同じパスの重複
- シンボリックリンク(Unix のモードがリンクを示すエントリ)
- 暗号化されたエントリ、deflate と無圧縮以外の圧縮方式
- 展開後の合計が 4 GiB を超える、またはエントリの圧縮率が 1000 倍を超える(zip 爆弾)

展開したファイルのモードは、zip の Unix のモードに実行ビットがあれば 0755、なければ 0644 にする(それ以外のビット、setuid などは捨てる)。Windows ではモードを無視する。

## 12. 言語ごとの注意

| 言語 | パッケージに入れるもの | 受け取る側に要るもの |
| --- | --- | --- |
| Rust | `plugins/<target>/` に so / dylib / dll | C ABI のプラグインだけなら、その ABI の版を読める juiz。Rust ABI のプラグイン(EC・ブローカーなど)を含むなら、`juiz` で指定された版で、同じ rustc でビルドされた juiz(§12.1) |
| C++ | プラグインと libjuiz_sdk(と自分で同梱したいライブラリ) | 対応する版の juiz |
| Haskell | プラグインと libjuiz_sdk と GHC の共有ライブラリ・libffi・libgmp(§12.2) | 対応する版の juiz |
| Python | `.py` | Python を埋め込める juiz(pip の juiz-cli が楽)と `python.requires` のパッケージ |

### 12.1 Rust のプラグインと rustc

Rust のプラグインには 2 種類ある(rust-plugin-abi.md、#291 で C ABI を足した)。

| 種類 | どれか | 受け取る側の juiz に要るもの |
| --- | --- | --- |
| C ABI | `#[juiz_process]` などのマクロで作った Process・Container・ContainerProcess・Component で、C ABI の入った juiz_sdk でビルドしたもの(印 `juiz_rust_abi=<版>;` を持つ) | その ABI の版を読めること。rustc と juiz_sdk の版は問わない |
| Rust ABI | EC・ブローカーのプラグイン、コンテナスタック、C ABI より前の juiz_sdk でビルドしたもの | 作った人と同じ rustc(`rustc -V` の出力)と同じ juiz_sdk の版(`BuildInfo` が両方を比べる) |

- pack は、C ABI のプラグインがあれば `rust.abi` を、Rust ABI のプラグインがあれば `rust.rustc` を書く(§6.3)。run は §9 の段 6 でホストと比べて、読み込む前に分かる文で止める。
- C ABI のプラグインだけのパッケージは、`cargo install juiz_app` で自分の rustc でビルドした juiz や、Homebrew の rustc でビルドした juiz でも動く。`juiz` の版の範囲も `>=作った版` でよい(§4.3)。
- Rust ABI のプラグインを含むパッケージは、今までどおり作った人と同じ rustc でビルドされた juiz でしか動かない。配る相手が多いアプリは、EC・ブローカーを自分で書かずに juiz に入っているものを使うか、配布用の juiz の rustc を揃える運用が要る。
- コンテナとそのコンテナプロセスを別のクレートで作るときは、同じ rustc・同じ juiz_sdk でビルドしなければならない(rust-plugin-abi.md §8.3)。同じパッケージの中なら作った人が同じ環境でビルドするので普通は満たされる。pack はこれを検査しない(合わなければ呼び出し時に `ContainerTypeMismatchError`)。

### 12.2 C++ / Haskell(#287)

C ABI のプラグインは libjuiz_sdk を通して値を受け渡すので、libjuiz_sdk を同じディレクトリ(`plugins/<target>/`)に置き、プラグインがそれを見つけられるようにする。

- Windows はプラグインの隣の DLL を探す(PR #274 で対応済み)。
- Linux / macOS はプラグインの rpath を `$ORIGIN` / `@loader_path` にする(#287)。
- 同梱した libjuiz_sdk は、今までどおり開いたときに `juiz_sdk_build_info` でホストと比べる。

作り方(#287):

- C++(CMake): `find_package(juiz_sdk)` の後で `juiz_sdk_bundle(<target>)` を呼ぶ。`<target>` の rpath を `$ORIGIN` / `@loader_path` だけにし(ビルドした場所でも SDK の絶対パスを書き込まない)、ビルドした `<target>` の隣に libjuiz_sdk(Windows は `juiz_sdk.dll`)を写す。できたディレクトリの中身を `plugins/<ターゲット名>/` に置く。
- Haskell など、ほかの共有ライブラリにも依存するもの: `scripts/bundle-plugin-deps.sh <プラグイン> <出力ディレクトリ>` で、プラグインと依存するライブラリを 1 つのディレクトリに集め、どれも隣を探すように書き換える(Linux は patchelf で RUNPATH を `$ORIGIN`、macOS は install_name_tool で `@rpath/<名前>` と `@loader_path`、書き換えた後に codesign で署名し直す)。Haskell のプラグインは libjuiz_sdk のほかに GHC の共有ライブラリ(`libHSbase-*`、`libHSrts*` など、cabal の `foreign-library` は base と RTS にも動的リンクする)と libffi・libgmp に依存するので、これで全部を集める。Windows は対象外(Haskell の Windows は #137)。
- 写さない「OS の基本のライブラリ」は、Linux は glibc(libc・libm・libdl・libpthread・librt・libutil・libresolv・ld-linux)と libgcc_s・libstdc++、macOS は `/usr/lib/` と `/System/` の下。受け取る側の glibc が作った側より古いと動かないので、Linux 向けは古めの環境(Ubuntu 22.04 など)でビルドする。
- `juiz pack` は、ホストのターゲットのネイティブのプラグインについて依存するライブラリを調べ(Linux は `ldd`、macOS は `otool -L`)、プラグインの隣にも OS の基本のライブラリにも無いもの(見つからない、またはパッケージの外で見つかった)を警告する。ほかのターゲットのプラグインは調べられないので、`scripts/bundle-plugin-deps.sh --check <プラグイン>` をその OS で使う。

## 13. 後回しにするもの

format 1 では行わない。足すときは format を上げるか、format 1 のまま足せる形(任意の項目・別ファイル)にする。

| もの | 今の扱い | 足すときの形の見込み |
| --- | --- | --- |
| 署名 | SHA-256 の照合だけ。改ざんは防がない(壊れと入れ忘れを見つける) | `juizapp.yaml.sig`(minisign か Sigstore)と、信頼する鍵の登録 `juiz trust add`。パスは予約済み(§3) |
| juiz を同梱した単体版 | 入れない | `bin/<target>/juiz` を入れる。パスは予約済み(§3) |
| conf の include(基礎とアプリを同じプロセスで動かす) | 行わない。基礎は別プロセス | conf に include を足し、依存先の conf をその場所相対で取り込む |
| 基礎の自動起動 | しない。§10.3 で `juiz run <名前>` を案内 | `juiz run` に起動の選択肢を足す |
| ネーミングサービス | IPC 名 = パッケージ名、http は固定ポート | 名前から接続先を引く仕組み(#289)。§10.2 の既定のつなぎ先をそれに置き換える |
| キャッシュの掃除 | しない | `juiz cache clean` など |
| 依存先の自動取得 | しない | パッケージの置き場(レジストリ)ができてから |

## 14. Step との対応

| Step | issue | この文書の節 |
| --- | --- | --- |
| 1 仕様書 | #283 | 全体 |
| 2 `{target}` とブローカーのパス | #284 | §5.2、§5.3 |
| 3 `juiz pack` と `juiz run` | #285 | §3、§4、§6、§7、§9、§11、§12.1 |
| 4 `juiz install` と依存、基礎 | #286 | §8、§10 |
| 5 C++ / Haskell の同梱 | #287 | §12.2 |
| 6 サンプルとサイト | #288 | — |
| Rust プラグインの C ABI(#291 の Step 4) | #332 | §4.1 の `rust.abi`・`rust.rustc`、§4.3、§6.3 の 6、§9 の段 6・10、§12.1(#328 で直し、#332 で実装) |
