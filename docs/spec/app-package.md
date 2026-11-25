# アプリ配布形式(.juizapp)仕様

- 状態: 確定(.juizapp Step 1、issue #283、2026-10-30)
- 親 issue: #282。検討の経緯と比較はプロジェクトの計画書 app-package-plan.md と検討資料(2026-10-30 決定)にある
- 関連: IPC ブローカー [ipc-broker.md](ipc-broker.md)(基礎パッケージの IPC 名)、データ型 [datatype.md](datatype.md)(接続時の型の指紋)、Rust プラグインの C ABI [rust-plugin-abi.md](rust-plugin-abi.md)(§4.1・§4.3・§6.3・§9・§12 の Rust の扱い、#328 で直した)
- 起動時の確認と対話・ポートが使用中のとき・関連付け(§15〜§17)は #390 の Step 0(#391、2026-11-17)で足した。計画はプロジェクトの計画書 juizapp-usability-plan.md(2026-11-02 確定)
- この文書は #284〜#288 の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と範囲

Process などのプラグインと conf をまとめた 1 つのファイルを配り、受け取った人が `juiz run` だけで動かせるようにする。

- 入れ物は zip、拡張子は `.juizapp`。ルートにマニフェスト `juizapp.yaml` を置く。
- 1 ファイルに複数の OS・アーキテクチャのバイナリを入れてよい。
- juiz 本体は入れない。受け取る側に juiz(cargo の `juiz` か pip の `juiz-cli`)が入っている前提にする。
- ロボットの基礎部分を「基礎パッケージ」(`kind: base`)として常駐させ、別のアプリ(`kind: app`)が別プロセスからそれにつなぐ形を扱う。

扱わないもの(§13 で後回しにしたもの)は、署名、juiz を同梱した単体版、conf の include による同一プロセス方式、基礎を対話なしで起動すること、ネーミングサービス。

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
| `languages` | 列 | ○(パッケージ内) | — | pack | `rust` / `python` / `c++` / `haskell` / `zig` のうち使っているもの |
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
   - ネイティブのプラグイン(`language` が `rust`(省略時)・`c++`・`haskell`・`zig` のもの、と `broker_factories`・`ec_factories`)は `path` に `{target}` を含まなければならない。
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
9. entry の conf の `containers`・`composite_containers` の各 Container の `model`(container-model.md §5.1・§10、#383)について:
   - `model.path` は相対パスで、entry の conf の親ディレクトリから解決した結果がパッケージの中を指していなければならない(`..` で外に出てはならない、絶対パスは不可)。
   - そのファイルが §6.2 で集めたファイル(除外規則の後)になければならない。
   - core の作成時の検査(container-model.md §5.2。キーの型、大きさの上限は entry の conf の `option.model.max_bytes`、形式の先頭の確かめ)に通らなければならない。`juiz run` で起動したときに同じ検査で止まるものを、配る前に見つけるため。
   - 形式が `sdf` なら、`<visual>` の `<geometry>` の `<mesh><uri>` は .sdf からの相対パスで、パッケージの中を指していなければならない(絶対パス・`..` で外に出るものはエラー)。指す先のファイルが pack に入らないとき、`uri` が `://` を含む(`model://`・`package://`・URL)ときは警告にとどめる(`<mesh>` は今は表示に使わない、container-model.md §3.2.1)。`<collision>` の `<mesh>` は見ない。
10. 各ファイルの SHA-256 を `files` に書く(パスの順に並べる)。
11. §6.4 のとおり zip を書く。

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
juiz run <パッケージ> [--entry <conf>] [--depend <名前>=<URL>|none]... [--remap <旧>=<新>]... [その他の juiz のオプション]
```

`<パッケージ>` は次のどちらか。

- `.juizapp` のファイルのパス。名前が `.juizapp` で終わるか、そのパスにファイルがあればこちら。
- インストール済みのパッケージの名前(`robot_base`、`robot_base@1.2.0`)。版を省けば、インストール済みの中で一番新しい版を使う(§8)。

### 7.1 動作

1. §9 の順に検証する。
2. ファイルなら §7.2 のキャッシュに展開する。インストール済みならその置き場所をそのまま使う。
3. 展開先の `entry`(`--entry` があればそれ)の conf を読み、普通の `juiz -f` と同じく conf の親ディレクトリを基準にパスを解決する。
4. `kind: base` なら §10.1、`depends` があれば §10.2 の設定を conf に足す。`--remap` があれば §7.3 のとおりトピック名を付け替える。
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
  `juiz run` 自身が受け付けるのは `--entry`・`--http`・`--no-http`・`--ipc`・`--namespace`・`--depend`・`--remap`(`--http`・`--no-http`・`--ipc`・`--namespace` は `juiz --http run …` のように run の前に書いてもよい)。
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

### 7.3 トピックの付け替え `--remap`(#452)

```
juiz run planner_dwa --depend mobile_sim=none --remap odom=nav/odom --remap cmd_vel=cmd_vel_dwa
```

配布されたアプリを、conf を展開して書き換えずに、別のトピック名で動かすためのもの(ほかのアプリの下に葉としてつなぐときなど)。

- `--remap <旧>=<新>` は繰り返せる。conf を読んだ後、起動する前に、conf の Process の次の項目でトピック名が `<旧>` のものを `<新>` にする。
  - トップレベルの `processes`、`containers` と `composite_containers` の `processes` の各要素の、`subscribes` の値と `publishes` の要素(文字列か、`name` を持つ辞書)。
  - それ以外(`transforms`、`connections`、プラグインのコードが自分で作る Topic など)は変えない。
- 名前は conf に書いたとおりの文字列で比べる(ネームスペースで解決する前)。`<新>` も conf に書いたのと同じく解決される(相対名ならその Process のネームスペースの下、先頭 `/` なら絶対)。
- 1 つの名前は 1 回だけ付け替える(`a=b` と `b=c` を渡しても `a` は `b` になる)。同じ `<旧>` を 2 度渡したら後のものを使う。
- 書式が違えば(`=` が無い、どちらかが空)起動前にエラー(`--remap must be <old topic>=<new topic> (ex., cmd_vel=cmd_vel_dwa), but '…'`)。
- conf に無い名前を渡してもエラーにしない。
- `juiz -f` には無い(conf を直接書き換えられるため)。

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

`juiz run` と `juiz install` は次の段を検証する。前の段で失敗したら、後の段は行わない。軽く、利用者が直しやすい原因から先に見る。

段の番号は見ることの区別に使う。行う順番は番号順ではなく、**マニフェストだけで決まる段 6〜8 を、ファイルを照合して展開する段 5 より前に行う**(#325)。ホストに合わないパッケージでは、展開もディスクへの書き込みもしない(キャッシュの `apps/` の下にも、`packages/<name>/` にも何も作らない)。

- `juiz run <file>`: 1 → 2 → 3 → 4 → 6 → 7 → 8 → 5 → 9 → 10
- `juiz install`: 1 → 2 → 3 → 4 → 6 → 5
- インストール済みの `juiz run <name>`: 2 → 3 → 4 → 6 → 7 → 8 → 9 → 10

| 段 | 見ること | 失敗したとき |
| --- | --- | --- |
| 1. 入れ物 | zip として開ける。ルートに `juizapp.yaml` がある(1 MiB 以下) | `<file> is not a juiz app package (no juizapp.yaml at the root)` |
| 2. format | `format` を知っている(今は `1` だけ) | `<file> uses package format <n>, but this juiz (<版>) supports up to 1. Update juiz.` |
| 3. juiz の版 | ホストの juiz の版が `juiz` の範囲に入る | `<name> <ver> requires juiz <範囲>, but this is juiz <版>.` に続けて、`=x.y.z` なら `Install it with: cargo install juiz_app --version x.y.z` か `pip install juiz-cli==x.y.z` |
| 4. ターゲット | ホストのターゲットが `targets` にあるか、`targets` が `[any]` | `<name> <ver> is built for <targets の並び>, but this machine is <ホスト>.` |
| 5. ファイル | §11 の zip の安全規則と、`files` との照合(§9.1) | 規則・ファイルごとの文(§11、§9.1) |
| 6. ビルド情報 | `rust.abi` があれば、ホストがその C ABI の版を読める。`rust.rustc` があれば、ホストの rustc と一致する(§12.1) | `<name> <ver> contains Rust plugins with plugin ABI <n>, but this juiz supports ABI <版の並び>. Update juiz.` / `<name> <ver> contains Rust plugins built with <rustc>, but this juiz was built with <rustc>.` |
| 7. 言語 | `python` を含むなら、ホストが Python を埋め込める(embed-python)。`python.version` に合う | `<name> needs Python, but this juiz was built without Python support. Install juiz with: pip install juiz-cli` など |
| 8. Python の依存 | `python.requires` の各要求を、ホストが埋め込む Python の `importlib.metadata` で満たせる | 足りないものを並べ、`<その Python の実行ファイル> -m pip install "numpy>=1.26" …` を案内。有効にした venv があれば `python -m pip install …`。埋め込みの Python では実行ファイルを `sys.base_prefix` から求める。その Python が externally-managed(PEP 668、Homebrew の Python など)なら、`<実行ファイル> -m venv .venv` → venv を有効にする → `python -m pip install …` → 有効にしたまま juiz を動かす、の手順を案内。Python のプラグインに要る `juiz` モジュールも無ければ、入れるものに `pyjuiz` を足す(#321) |
| 9. 依存 | §10.2 | §10.3 |
| 10. 読み込み | 今までどおり、プラグインを読むときの検査(Rust の C ABI は表の ABI の版(rust-plugin-abi.md §9.2)、Rust ABI は `check_build_info`、C++ / Haskell は `juiz_sdk_build_info`)と、接続時の型の指紋(datatype.md §8・§19) | 既存のエラー |

- `juiz install` は 1〜6 を行う(7〜9 は動かす機械で決まるので `juiz run` のときに見る)。段 6 は展開(段 5)の前に行う。
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
- system の profile(`system/profile`)に次を載せる。アプリはこれで相手を確かめる。

```json
"package": { "name": "robot_base", "version": "1.2.0", "kind": "base" }
```

`kind: app` を `juiz run` したときも `package` を載せてよい(管理画面での表示用)。パッケージを使わずに起動したときは載せない。

### 10.2 アプリを起動するとき(#286)

`depends` の各 `<名前>: <範囲>` について、つなぎ先を次の順で決め、subsystem として conf に足す。

1. コマンドラインの `--depend <名前>=<URL>`(`ipc://robot_base2`、`http://192.168.0.10:8100`、`http://192.168.0.10`)。`--depend <名前>=none` なら、その依存にはつながない(下、#452)
2. アプリの conf の `subsystems` のうち、`package: <名前>` を持つもの
3. 既定: `{type_name: ipc, name: <名前>}`(`ipc://<名前>`)

#390 で、1 と 2 の間に「§15.6 で覚えたつなぎ先」が入る。

```yaml
# アプリの conf で別の機械の基礎を使う例
subsystems:
  - package: robot_base       # この subsystem を depends の robot_base として扱う
    type_name: http
    name: "192.168.0.10"      # ポートを省くと、インストール済みの robot_base の base.http_port
```

- `type_name: http` でポートを省いたときは、インストール済みの基礎(範囲に合う一番新しい版)の `base.http_port` を使う。基礎が入っていないか `base.http_port` が無ければエラー(`cannot decide the port for robot_base. Write the port in the conf or --depend.`)。
- `package` を持たない `subsystems` は今までどおり扱い、依存の確認はしない。
- `--depend <名前>=none`(#452): その依存を subsystem に足さず、conf の `package: <名前>` の subsystem も取り除き、確認(§10.3)もしない。基礎が入っていなくても、動いていなくても起動する。別のアプリの下に葉としてつなぐときなど、依存先へ自分からつながない起動のため(§7.3 の `--remap` と合わせて使う)。
  - conf に `package://<名前>/…` の ID があればエラー(`'package://robot_base/…' in the conf refers to package robot_base, but --depend robot_base=none was given`)。`ipc://<名前>/…` は書き換えない(§10.4)。
  - 同じ名前の `--depend` を複数渡したら後のものを使う(`none` の後に URL を渡せばつなぐ)。
- conf に書いた依存先の ID(`transforms` のつなぎ先など)も同じ規則で決まったつなぎ先にする(§10.4)。
- つないだ後、相手の profile の `package` を見て、`name` が一致し、`version` が範囲に入ることを確かめる(§10.3)。
- subsystem としてつなぐと、相手は同じ種類のブローカーでこちらへつなぎ返す(mastersystem)。そのため `juiz run` は、アプリ側にもブローカーを用意する。
  - ipc でつなぐとき、conf に ipc ブローカーが無く `--ipc` も無ければ、`<アプリ名>-<プロセス ID>` の名前で ipc ブローカーを足す(同じアプリを複数動かせるように)。
  - http でつなぐときは、`--http` が無くても HTTP ブローカーを立てる(ポートは conf の `option.http_broker.port`、既定 8000)。`--no-http` とは両立しないのでエラーにする(§7.1)。
- 確認(つないで profile を見る)は、アプリのブローカーを立てる前に行う。確認に失敗したときにソケットなどを残さないため。juiz_core の `System::set_subsystem_check` で、conf の `subsystems` の各要素について呼ぶ。
- 基礎とアプリの名前の衝突を避けるには、ネームスペースを分ける(基礎 `/robot1/base`、アプリ `/robot1/patrol` など)。juiz run はネームスペースを自動では付けない。

### 10.4 conf の ID のつなぎ先(#425)

アプリの conf に、依存する基礎の Process・Container をブローカー付きの ID で書くとき(System をまたぐ Transform の始点・終点(transformations.md §19.2)、Connection の端、EC の bind)、つなぎ先は §10.2 の `subsystems` と同じ規則で決める。`juiz run` は §10.2 でつなぎ先を決めた後、conf の次の項目を書き換える。

- 対象: `transforms` の `source`・`destination`、`connections` の `source`・`destination`、`ecs` の `bind`。値が文字列か、`identifier` / `id` を持つ辞書のもの。
- **`package://<名前>/<ID の残り>`**(推奨): `<名前>` は `depends` の名前。§10.2 で決まったつなぎ先の URL に置き換える(`package://mobile_sim/container/odom0::static_anchor` → `ipc://mobile_sim/container/odom0::static_anchor`、`--depend mobile_sim=http://192.168.0.10:8100` なら `http://192.168.0.10:8100/container/odom0::static_anchor`)。conf が特定のつなぎ先に縛られない。
  - `<名前>` が `depends` に無ければエラー(`'package://arm/…' in the conf refers to package arm, but my_patrol does not depend on arm. Add it to 'depends' in juizapp.yaml.`)。`package://<名前>` の後に ID が無ければエラー。
- **`ipc://<名前>/<ID の残り>`**(既定のつなぎ先をそのまま書いたもの、移行のため): `<名前>` が `depends` の名前なら、同じく決まったつなぎ先の URL に置き換える。既定のままなら変わらない。
- それ以外(依存でない名前の `ipc://`、`http://…` などを直に書いたもの、名前)は書き換えない。
- `package://` は `juiz run` だけが解決する。`juiz -f` で conf を直接読んだときは解決しない(ブローカーが見つからないエラーになる)。

```yaml
transforms:
  - source: slam_odom0
    destination: package://mobile_sim/container/odom0::static_anchor
    rate: 20.0
```

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
- 基礎を対話なしでは起動しない(§13)。対話できるときは、止める前に §15.4 の選択(基礎を起動する・つなぎ先を選び直す)を出す(#390)。

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
| Zig | プラグインと libjuiz_sdk(§12.2)。1 台から複数のターゲットの分を作れる(§12.3) | 対応する版の juiz |
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
- EC のプラグインは juiz_core に依存する。juiz_core の既定の feature には `embed-python` があり、そのままでは EC のプラグインが libpython にリンクして、パッケージの外の Python に依存する(pack が警告する、#322)。EC のプラグインは `juiz_core = { version = "…", default-features = false, features = ["swagger-ui", "webui", "ipc-broker"] }` のように `embed-python` だけを外して依存し(ほかの feature は構造体のフィールドを変えるので、Rust ABI で渡す型の形をホストの juiz と揃える)、`cargo build -p <EC のクレート>` のように単独でビルドする。同じワークスペースで juiz_app などと一緒にビルドすると、Cargo が feature を合わせるので `embed-python` が入る(ecs/ の EC もワークスペース全体の `cargo build` では libpython にリンクする)。

### 12.2 C++ / Haskell(#287)

C ABI のプラグインは libjuiz_sdk を通して値を受け渡すので、libjuiz_sdk を同じディレクトリ(`plugins/<target>/`)に置き、プラグインがそれを見つけられるようにする。

- Windows はプラグインの隣の DLL を探す(PR #274 で対応済み)。
- Linux / macOS はプラグインの rpath を `$ORIGIN` / `@loader_path` にする(#287)。
- 同梱した libjuiz_sdk は、今までどおり開いたときに `juiz_sdk_build_info` でホストと比べる。

作り方(#287):

- C++(CMake): `find_package(juiz_sdk)` の後で `juiz_sdk_bundle(<target>)` を呼ぶ。`<target>` の rpath を `$ORIGIN` / `@loader_path` だけにし(ビルドした場所でも SDK の絶対パスを書き込まない)、ビルドした `<target>` の隣に libjuiz_sdk(Windows は `juiz_sdk.dll`)を写す。できたディレクトリの中身を `plugins/<ターゲット名>/` に置く。
- Haskell など、ほかの共有ライブラリにも依存するもの: `scripts/bundle-plugin-deps.sh <プラグイン> <出力ディレクトリ>` で、プラグインと依存するライブラリを 1 つのディレクトリに集め、どれも隣を探すように書き換える(Linux は patchelf で RUNPATH を `$ORIGIN`、macOS は install_name_tool で `@rpath/<名前>` と `@loader_path`、書き換えた後に codesign で署名し直す)。Haskell のプラグインは libjuiz_sdk のほかに GHC の共有ライブラリ(`libHSbase-*`、`libHSrts*` など、cabal の `foreign-library` は base と RTS にも動的リンクする)と libffi・libgmp に依存するので、これで全部を集める。Windows は対象外(Haskell の Windows は #137)。
- Zig(zigjuiz の build.zig、#577): `juiz.addPlugin` で作ったライブラリを `juiz.installBundle(lib, .{})` に渡す。rpath を `$ORIGIN` / `@loader_path` だけにし(zig がリンクしたライブラリのディレクトリを足す rpath も止める)、`zig build` で `zig-out/plugins/` にプラグインと libjuiz_sdk を並べて置く。C++ の `juiz_sdk_bundle` と同じく、その中身を `plugins/<ターゲット名>/` に置く。zigjuiz は libc のほかに依存しないので、Haskell のような集め直しは要らない。Windows(#579)は `<名前>.dll` と `juiz_sdk.dll`(アーカイブの `bin/`)を並べる。Windows に rpath は無く、juiz がプラグインを `LOAD_WITH_ALTERED_SEARCH_PATH` で開くので隣の `juiz_sdk.dll` が読み込まれる。
- 写さない「OS の基本のライブラリ」は、Linux は glibc(libc・libm・libdl・libpthread・librt・libutil・libresolv・ld-linux)と libgcc_s・libstdc++、macOS は `/usr/lib/` と `/System/` の下。受け取る側の glibc が作った側より古いと動かないので、Linux 向けは古めの環境(Ubuntu 22.04 など)でビルドする。
- `juiz pack` は、ホストのターゲットのネイティブのプラグインについて依存するライブラリを調べ(Linux は `ldd`、macOS は `otool -L`)、プラグインの隣にも OS の基本のライブラリにも無いもの(見つからない、またはパッケージの外で見つかった)を警告する。ほかのターゲットのプラグインは調べられないので、`scripts/bundle-plugin-deps.sh --check <プラグイン>` をその OS で使う。

### 12.3 Zig のクロスビルド(#577)

Zig は 1 台から別の OS・CPU 向けにビルドできるので、Zig のプラグインだけのパッケージは、1 台で複数のターゲットの `plugins/<ターゲット名>/` を作れる。

1. そのターゲットの libjuiz_sdk を入れる: `juiz sdk install --target <ターゲット名>`(既定の置き場所は `~/.juiz/sdk/<版>-<ターゲット名>`、`juiz sdk path --target <ターゲット名>` で表示)。ターゲット名は Releases に置いている 5 種(`x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu`、`x86_64-apple-darwin`、`aarch64-apple-darwin`、`x86_64-pc-windows-msvc`)。
2. `zig build --release -Dtarget=<Zig のターゲット> -Djuiz-sdk=<1 の場所>`。Zig のターゲットは `aarch64-linux-gnu`・`x86_64-linux-gnu`・`aarch64-macos`・`x86_64-macos`・`x86_64-windows`(#588)。できた `zig-out/plugins/` を `plugins/<ターゲット名>/` に置く。`juiz pkg create -b zig` の雛形では `scripts/dev.sh --release --target <ターゲット名>` が 1〜2 をまとめて行う(pkg.md §6.6)。
3. 全部のターゲットのディレクトリを置いてから `juiz pack` する。pack はホストのターゲットのプラグインだけ依存するライブラリを調べる(§12.2)。

確かめたこと(2026-11-22、Linux x86_64 の Zig 0.17.1、libjuiz_sdk 0.3.2 の Releases のアーカイブ): `aarch64-unknown-linux-gnu`(RUNPATH `$ORIGIN`、`libjuiz_sdk.so` を NEEDED)と `aarch64-apple-darwin`(LC_RPATH `@loader_path`、`@rpath/libjuiz_sdk.dylib` を読み込む)のプラグインができ、ホストの分と一緒に 1 つの .juizapp(`targets` に 3 つ)にできた。別の機械での読み込みはこのクラウド環境では確かめていない(macOS の CI で確かめるのは #579)。

確かめたこと(2026-11-22、#579、同じ環境): `x86_64-windows`(Zig 既定の gnu)向けに examples/zig の 19 個がビルドでき、libjuiz_sdk 0.3.2 の `x86_64-pc-windows-msvc` のアーカイブの `lib/juiz_sdk.dll.lib` にそのままリンクできた(`juiz_sdk.dll` を読み込み、`manifest_entry_point` などを公開する DLL)。Windows・macOS の実機での読み込みは test-windows.yml・test-macos.yml(main への push と手動実行)で確かめる。

- Linux 向けは、Zig が持っている glibc の既定の版(ホスト向けのビルドはホストの glibc)にリンクする。受け取る側の glibc がそれより古いと動かない(§12.2 と同じ注意)。古い glibc に合わせるときは `-Dtarget=x86_64-linux-gnu.2.31` のように版を付ける。
- macOS 向けは Xcode が無くてもリンクできる(Zig が libSystem の定義を持っている)。arm64 は ad-hoc 署名される。
- Windows 向け(`x86_64-pc-windows-msvc` の libjuiz_sdk に `x86_64-windows-gnu` でリンクする)は #579 で確かめる。

## 13. 後回しにするもの

format 1 では行わない。足すときは format を上げるか、format 1 のまま足せる形(任意の項目・別ファイル)にする。

| もの | 今の扱い | 足すときの形の見込み |
| --- | --- | --- |
| 署名 | SHA-256 の照合だけ。改ざんは防がない(壊れと入れ忘れを見つける) | `juizapp.yaml.sig`(minisign か Sigstore)と、信頼する鍵の登録 `juiz trust add`。パスは予約済み(§3) |
| juiz を同梱した単体版 | 入れない | `bin/<target>/juiz` を入れる。パスは予約済み(§3) |
| conf の include(基礎とアプリを同じプロセスで動かす) | 行わない。基礎は別プロセス | conf に include を足し、依存先の conf をその場所相対で取り込む |
| 基礎の自動起動 | 対話なしではしない。§10.3 で `juiz run <名前>` を案内し、対話できるときは §15.4 の選択から起動できる(#390) | 対話なしで起動する既定動作 |
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
| conf の ID のつなぎ先 | #425 | §10.4 |
| `--remap` と `--depend <名前>=none` | #452 | §7、§7.3、§10.2 |
| Zig の同梱とクロスビルド | #577 | §12.2、§12.3 |
| Rust プラグインの C ABI(#291 の Step 4) | #332 | §4.1 の `rust.abi`・`rust.rustc`、§4.3、§6.3 の 6、§9 の段 6・10、§12.1(#328 で直し、#332 で実装) |
| 段 6〜8 を展開の前に行う | #325 | §9 |
| Container のモデルの検査(ContainerModel Step 3) | #383 | §6.3 の 9 |
| .juizapp 使い勝手 Step 0 仕様書 | #391 | §15、§16、§17 |
| 〃 Step 1 ブローカーの `port_fallback`・`adopt_master` と bind エラー | #392 | §16 |
| 〃 Step 2 対話の部品とポート・IPC 名の事前確認 | #393 | §15.1〜§15.3 |
| 〃 Step 3 親につながらないときの選択とつなぎ先の記憶 | #394 | §15.4〜§15.7 |
| 〃 Step 4 Windows の関連付けとコンソール窓 | #395 | §17(Windows、§17.2、§17.3) |
| 〃 Step 5 macOS・Linux の関連付けと SIGHUP | #396 | §17(macOS・Linux) |

## 15. 起動時の確認と対話(#390)

.juizapp を受け取った人が、ポートの衝突や基礎が無いことで止まったときに、その場で選び直せるようにする。決めごとは計画書 juizapp-usability-plan.md(2026-11-02 確定)による。実装は §15.1〜§15.3 が #393、§15.4〜§15.6 が #394。ブローカー側の設定は §16(#392)。

### 15.1 対話できるとき

`juiz run` は、次をすべて満たすときだけ利用者に選択を出す(以下「対話できる」)。

- 標準入力と標準エラーが両方とも端末である(Rust の `std::io::IsTerminal`)。
- `--yes` も `--no-prompt` も付いていない。

選択は標準エラーに番号つきで出し、標準入力から 1 行読む(OS のダイアログは使わない。ダブルクリックで起動したときもコンソール窓が開くので、そこで選べる。§17.3)。

```
port 8080 is in use by another juiz (talker_listener 0.1.0).
  1) start on port 8081 (free)
  2) enter a port number
  3) quit
choose [1]:
```

- 空行は角括弧の既定(推奨)を選んだことにする。範囲外・数でない入力は、同じ選択をもう一度出す。
- 標準入力が閉じた(EOF)・Ctrl+C のときは「やめる」と同じ(終了コード 1)。
- 選択肢の文は英語で出す(ログ・エラーの文と同じ)。
- `--yes` は「各選択の既定(推奨)を選ぶ」、`--no-prompt` は「対話できないときと同じ扱い」。違いは §15.2・§15.4 の表の「対話できないとき」と既定が違う所だけに出る。両方付けたら `--no-prompt` に従う。
- `juiz -f` には対話を足さない(conf を直接動かす人は §16 の設定で決める)。

### 15.2 ポートと IPC 名の事前確認

`juiz run` は `System::setup` の前(§9 の段 9 の後)に、立てるブローカーの待ち受けを試しに取ってすぐ放し、使用中かを確かめる。

- 対象: HTTP ブローカー(立てるとき、§7.1)のポート、qmp ブローカー(conf にあるとき)のポート、`kind: base` の IPC 名(§10.1)。アプリの IPC 名(`<アプリ名>-<プロセス ID>`)は衝突しないので見ない。
- 使用中なら相手を調べる(§16.3 の「相手が juiz か」)。juiz なら `system/profile` の `package` も読む。
- 基礎の IPC 名が使用中なら、相手を問わず `robot_base is already running at ipc://robot_base.` と出して終了する(終了コード 1。二重起動しない)。

ポートが使用中のときの扱い:

| 相手 | アプリ(`kind: app`) | 基礎(`kind: base`) |
|---|---|---|
| 同じパッケージの juiz(`package.name` が同じ) | 「`<name> is already running at http://127.0.0.1:<port>/`」→ 1) 空いている `<n>` でもう 1 つ起動 / 2) 動いている方の管理画面をブラウザで開いて終了 / 3) やめる | 「`<name> is already running`」で終了(二重起動しない) |
| 別の juiz | 1) 空いている `<n>` で起動 / 2) 番号を入力 / 3) やめる | 1) 空いている `<n>` で起動 / 2) やめる |
| juiz 以外 | 別の juiz と同じ | 別の juiz と同じ |
| 対話できないとき | 空いている番号で起動し、§16.4 の警告を出す(`port_fallback: error` か `--no-port-fallback` ならエラー) | エラー(`port 8100 for robot_base is in use by …. Free the port, or start it with --http-port <n>.`) |

- 「空いている `<n>`」は元の番号から +1 ずつ試して最初に取れた番号(上限は元の番号 + 100、無ければその選択肢を出さない)。
- 基礎の番号を変えるときは `note: apps that connect to robot_base over http look for port 8100 and will not find it.` を添える(§10.2 の http のつなぎ先は `base.http_port` で探すため)。
- アプリで、相手が juiz で §16.2 の `adopt_master` が有効なら、1) の文に `(join it as its subsystem)` を添え、選んだら相手を conf の `mastersystem` に足して起動する(§16 の「相手を親とみなす」と同じ結果を、事前確認で決める)。
- 選んだ番号は今回だけ使い、覚えない。固定したいときは conf の `option.http_broker.port` か `--http-port <n>` を使う。
- 番号を選び直した結果、§16 の取り直しは起きない(事前確認で取れた番号を conf に入れて起動するため)。事前確認の後に誰かが取った場合は §16 の規則で扱う。
- qmp のポートも同じ表で扱う(選択肢の文の `port` を `qmp port` にする)。

#393 の実装で決めた細部:

- 対象のブローカーは `option.http_broker`(立てるとき。番号を書いていなければ 8000)と、`brokers` の `type_name: http`・`qmp`(書いていなければ 8080)。どれも `port_fallback`・`adopt_master` はそのブローカーの設定(§16.1、§16.2 の既定と引数を入れた後)で見る。
- アプリで `port_fallback: error`(か `--no-port-fallback`)なら「空いている `<n>` で起動」を出さない。番号の入力とやめるは出す(入れた番号は利用者が選んだものなので取り直しには当たらない)。
- 入れた番号も使用中なら、その番号について同じ選択をもう一度出す。「空いている `<n>`」はそこから +1 ずつ探す。
- 同じパッケージでも qmp のポートでは 2)(管理画面を開く)を出さない。
- `--yes` は一番上の選択が「空いている `<n>` で起動」のときだけそれを選ぶ。そうでないとき(空きが無い、`port_fallback: error`)は、アプリは対話できないときと同じ扱い、基礎は `no free port for robot_base in 8100..8200` のエラーにする。
- 基礎のポートに同じパッケージの juiz がいたときの文は `robot_base is already running at http://127.0.0.1:8100/`(終了コード 1)。対話できないときの基礎の qmp のエラーは、`--http-port` の代わりに `change the port in the conf` と案内する。
- 相手を親にすると決めたときは `joining the juiz at 127.0.0.1:8080 as its subsystem` と出し、conf の `mastersystem` に §16 と同じ値(`host`・`port`・`broker_type`・`broker_name`)を入れる。2 つ目以降のブローカーで juiz に当たっても親は増やさない。
- 「やめる」と入力の EOF は何も出さずに終了コード 1。「管理画面を開いて終了」は終了コード 0。ブラウザは Windows が `cmd /C start`、macOS が `open`、それ以外が `xdg-open` で開き、開けなければ URL を出す。

### 15.3 `juiz run` に足すオプション

| オプション | 意味 |
|---|---|
| `--http-port <n>` | HTTP ブローカーのポートを `<n>` にする(conf の `option.http_broker.port` より優先)。HTTP ブローカーを立てるかは変えない(§7.1)。`kind: base` で `base.http_port` と違う番号を渡すと §15.2 の `note:` を出す |
| `--yes` / `-y` | 選択を出さず、各選択の既定を選ぶ(§15.1) |
| `--no-prompt` | 選択を出さず、対話できないときの扱いにする(§15.1) |
| `--no-port-fallback` / `--no-adopt-master` | §16.2 |
| `--forget-depends` | §15.6 の覚えたつなぎ先を、このアプリの分だけ消してから起動する |
| `--launched` | 関連付けから起動された印(§17.3)。利用者が打つものではない |

起動できたら、立てたブローカーのつなぎ先を標準エラーに 1 行ずつ出す(例 `HTTP broker: http://127.0.0.1:8081/ (web UI: /ui)`、`IPC broker: ipc://robot_base`)。実際の番号は `system/profile` のブローカーの名前でも分かる。

### 15.4 親(基礎)につながらないとき

§10.2 の確認(`subsystem_check`)で、つなげないとき(§10.3 の表の上 2 行)、対話できるなら次を出す。つながったが相手が違う・版が合わないとき(表の下 3 行)も、理由の文を出した後に同じ選択を出す。

```
my_patrol depends on robot_base >=1.2, <2, but it is not running at ipc://robot_base.
  1) start robot_base 1.3.0 now                 ← 範囲に合う版がインストール済みのときだけ
  2) choose a running robot_base                ← §15.5 で見つかったときだけ
  3) enter where robot_base is (ex., http://192.168.0.10:8100)
  4) retry
  5) quit
choose [1]:
```

- 既定は、出ている選択肢のうち一番上のもの。
- 1) は `juiz run robot_base@<版>` を **別プロセス・別窓** で起動する(§15.7)。つながるまで最大 10 秒、0.5 秒おきに試し、つながったら続ける。つながらなければ `robot_base did not start in 10 s.` と出して同じ選択に戻る。アプリを止めても基礎は止めない。
- 2) は見つかったものを `1) robot_base 1.3.0 at ipc://robot_base2` の形で並べ、選ばせる。
- 3) は `--depend` と同じ書式の URL を受け、§10.2 と同じくつなぎ直して §10.3 の確認を通す。失敗したら理由を出して同じ選択に戻る。
- 2) 3) でつながったら、§15.6 に覚える。
- 依存が複数あれば、1 つずつ順に聞く。
- 対話できないときは §10.3 のエラーで止まる(文は変えない)。`--yes` のときも同じ(基礎を対話なしで起動する既定は持たない、§13)。
- `--depend <名前>=…` を渡した依存も、つながらなければ同じ選択を出す。覚えるのは 2) 3) で選んだときだけ。

### 15.5 動いている基礎を探す(この機械の中だけ)

- IPC の待ち受けを列挙する。Unix は ipc-broker.md §2.2 のソケットの置き場所にあるソケットのファイル、Windows は `\\.\pipe\` の下の `juiz-` で始まる名前付きパイプ。
- 見つかった名前それぞれに `system/profile` を問い合わせ(1 つ 1 秒まで)、`package.kind` が `base`、`package.name` が依存の名前と一致し、版が範囲に入るものを候補にする。
- HTTP・qmp で動いている基礎は探さない。他の機械を探すのはネーミングサービス(#289)の仕事にする。
- #289 で、この探し方を機械ごとの登録の一覧に置き換え、つなぎ先の順(§15.6)に登録を引く段を足す。決めごとは [naming.md](naming.md) §8(Step 2 #617 で実装)。

### 15.6 選んだつなぎ先を覚える

- §15.4 の 2) 3) でつながった先を `<データ>/depends.yaml`(`<データ>` は §8.1)に書く。

```yaml
# <データ>/depends.yaml  (juiz run が書く。手で直してもよい)
my_patrol:                 # アプリのパッケージ名
  robot_base: http://192.168.0.10:8100
```

- §10.2 のつなぎ先の順を次にする: 1. `--depend` → **2. 覚えた先** → 3. conf の `subsystems[].package` → 4. `ipc://<名前>`。
- 覚えた先を使ったときは `using robot_base at http://192.168.0.10:8100 (remembered; --forget-depends to reset)` と標準エラーに出す。
- 覚えた先につながらなければ、§15.4 の選択を出す(対話できなければ §10.3 のエラーに、上の文を添える)。
- `--forget-depends` はそのアプリの項目を消す。ファイルが壊れていたら警告を出して無いものとして扱う。
- 書くのはアプリ名ごと。同じ基礎に依存する別のアプリには効かない。

### 15.7 基礎を別窓で起動する

§15.4 の 1) で基礎を起動するとき、子プロセスとして待たない(アプリが止まっても残す)。

| OS | 方法 |
|---|---|
| Windows | `CREATE_NEW_CONSOLE` で `"<juiz>" run --launched robot_base@<版>` を起動する |
| macOS | `osascript` で Terminal に `"<juiz>" run --launched robot_base@<版>` を打たせる |
| Linux | `$TERMINAL`、`x-terminal-emulator`、`gnome-terminal`、`konsole`、`xterm` の順で見つかったものの `-e` で起動する。表示が無い(`DISPLAY`・`WAYLAND_DISPLAY` が無い)か見つからなければ、端末から切り離して起動し、出力を `<キャッシュ>/logs/robot_base.log` に書き、そのパスを出す |

- `<juiz>` は §17.2 の「juiz の実行ファイル」と同じ求め方をする。
- 起動した基礎には `--launched` を付ける(エラーで止まったら窓が残って読める、§17.3)。

#394 の実装で決めた細部:

- 確認は `System::setup` の前に `juiz run` の中で行う(§10.2 の `subsystem_check` は setup の中にそのまま残り、もう一度確かめる)。選び直した先は `--depend <名前>=<URL>` を渡したのと同じ扱いで、§10.2 から conf を作り直す。そのため §10.4 の ID の書き換えと、つなぎ返しのためのブローカー(ipc の名前・HTTP)も選び直した先に合う。
- 起動前に確かめるのは `ipc`・`http`・`qmp` のつなぎ先だけ。他の種類は setup の確認に任せる。`--no-http` と http での接続が両立しないエラー(§7.1)は、この確認より前に、`--depend` と conf で決まる先で出す。
- つながったが §10.3 の下 3 行に当たるときは、その文を出した後、選択の問いを `my_patrol depends on robot_base >=1.2, <2.` にする。
- 1) は今のつなぎ先でつながるのを待つ。起動して `robot_base did not start in 10 s.` になったとき、または相手が §10.3 の確認に通らないときは同じ選択に戻る。1) でつながった先は覚えない。
- 2) は今のつなぎ先を除いて並べる。並べた中から選ぶ問いは `running robot_base:`。
- 3) の問いは `where is robot_base: `。空行なら同じ選択に戻る。書式の誤り・ポートを決められない(§10.2)ときは理由を出して同じ選択に戻る。覚えるのは §10.2 でポートを補った後の URL(`http://192.168.0.10` と入れても `http://192.168.0.10:8100` を覚える)。
- `--depend` で渡した依存を 2) 3) で選び直したときも覚える。次に `--depend` 無しで起動すると、覚えた先を使う。
- 覚えた先の文(§15.6)は、つながったことを確かめてから出す。つながらず対話できるときは、選択の前に出す。
- `--forget-depends` で消したときは `forgot the remembered connections of my_patrol` と出す。`depends.yaml` はキーも値も二重引用符で書き、隣の一時ファイルに書いてから置き換える。
- §15.7 の Linux: `gnome-terminal` だけは `-e` ではなく `--` の後に並べる。端末から切り離すときは別のプロセスグループで起動する(アプリの端末の Ctrl+C が届かない)。`<キャッシュ>` は §7.2。別窓で起動したときは `starting robot_base 1.3.0 in another window.`、切り離したときは `starting robot_base 1.3.0 in the background; its output goes to <ログのパス>` と出す。
- `--launched` は #394 では受け付けるだけ(画面に出ない引数)。§17.3 の動作は #395 で入れる。

## 16. ブローカーのポートが使用中のとき(#390)

HTTP ブローカーは、待ち受けるポートが使用中なら +1 して取り直し、相手が juiz ならその子(subsystem)になる(相手を `mastersystem` とする)動作を前から持っている。これを既定として残し(2026-11-02 決定)、それぞれを禁止できるようにする。実装は #392。

### 16.1 conf

```yaml
option:
  http_broker:
    port: 8080
    port_fallback: next     # next(既定)| error
    adopt_master: true      # true(既定)| false
brokers:
  - type_name: qmp
    port: 3000
    port_fallback: next     # qmp も同じ 2 つを持つ
    adopt_master: true
```

| 設定 | 値 | 意味 |
|---|---|---|
| `port_fallback` | `next`(既定) | 使用中なら +1 ずつ、元の番号 + 100 まで取り直す |
| | `error` | 使用中なら起動を止める(§16.4 のエラー) |
| `adopt_master` | `true`(既定) | 使用中の相手が juiz なら、その System を `mastersystem` とする |
| | `false` | 相手が juiz でも親とみなさない(番号の取り直しだけ) |

- 知らない値はエラー(`port_fallback must be 'next' or 'error', but '…'`)。
- conf に `mastersystem` を書いていれば、`adopt_master` によらずそちらを使う(相手を親にしない)。
- `adopt_master` は `port_fallback: next` のときだけ意味を持つ(`error` なら相手につながる前に止まる)。

### 16.2 コマンドの引数

`juiz -f` と `juiz run` の両方に足す。conf より優先する。

| 引数 | 効き目 |
|---|---|
| `--no-port-fallback` | すべてのブローカーを `port_fallback: error` にする |
| `--no-adopt-master` | すべてのブローカーを `adopt_master: false` にする |

`juiz run` の既定(conf にも引数にも無いとき)は次のとおり。

- `kind: base`: `adopt_master: false`(基礎がよその juiz の子になることはない)。`port_fallback` は §15.2 の事前確認で決まる。
- `kind: app` で §10.2 によって依存の subsystem を 1 つ以上足したとき: `adopt_master: false`。依存先の基礎がつなぎ返して `mastersystem` になるので、たまたまポートにいた juiz も親にすると親が 2 つになるため(#391 で決めた)。conf に `adopt_master: true` と書けばそれに従う。
- それ以外の `kind: app`: §16.1 の既定のまま。

### 16.3 相手が juiz か

ポートが使用中のとき、相手が juiz かを次で確かめる。

- HTTP: `GET http://127.0.0.1:<port>/api/system/profile` を 1 秒のタイムアウトで呼び、200 で JSON のオブジェクトが返り、`system_store.uuid` を持てば juiz とみなす。
- qmp: そのポートへ qmp の BrokerProxy でつなぎ、`system/profile` を同じ規則で確かめる(1 秒)。
- それ以外(つながらない、タイムアウト、形が違う)は juiz 以外とみなす。juiz 以外は親にしない(`port_fallback: next` なら番号を取り直すだけ)。
- 0.2.11 までの `check_server_is_juiz` は常に真を返す仮実装で、無関係なサーバーを親とみなしていた。#392 でこの規則に置き換えた。
- HTTP で `system/profile` が 404 なら、0.2.10 までの juiz として `system/profile_full` で同じ規則を確かめる(#508)。
- 親とみなすのは、取り直しの途中で最初に見つけた juiz だけ。

### 16.4 利用者への表示とエラー

ログ(`log::info` など)は既定で見えないので、次は標準エラーに直接出す。

| 場合 | 文 |
|---|---|
| 番号を取り直した | `warning: port 8080 is in use; the HTTP broker uses 8081` |
| 相手を親とみなした | `joined the juiz at 127.0.0.1:8080 as its subsystem` |
| `port_fallback: error` で使用中 | エラー `port 8080 for the HTTP broker is in use (by a juiz / by another program). Use another port, or remove port_fallback: error / --no-port-fallback.` |
| 上限まで空きが無い | エラー `no free port for the HTTP broker in 8080..8180` |
| 使用中以外の bind の失敗(権限、アドレスが無いなど) | エラー `cannot start the HTTP broker on 0.0.0.0:80: <OS の理由>` |

- 使用中以外の bind の失敗を黙って捨てない。今は log に出すだけで、`start_brokers` の待ちが `TimeoutError` になり原因が見えない。ブローカーの起動の失敗として `start_brokers` のエラーに理由を載せる。
- qmp も同じ文(`HTTP broker` を `qmp broker` にする)。

## 17. ファイルの関連付けとダブルクリック起動(#390)

`.juizapp` をダブルクリックで起動できるようにする。インストーラは作らず、`pip install juiz-cli`(か `cargo install juiz_app`)の後に利用者が 1 回だけ `juiz associate` を打つ。実装は Windows が #395、macOS・Linux が #396。

### 17.1 コマンド

```
juiz associate            # 今の juiz を .juizapp に関連付ける(この利用者だけ)
juiz associate --remove   # 外す
```

- 管理者権限は要らない(利用者ごとの場所にだけ書く)。
- 既に別の juiz が関連付けられていたら、上書きする前に `.juizapp was associated with <前のパス>; now <今のパス>` と出す。同じなら何もせず成功。
- venv の中で打つと、その venv の juiz が登録される(その venv の Python とパッケージで動く)。
- `--remove` は、関連付けが今の juiz を指していなくても、§17.4 で書いたものを消す。無ければ何もせず成功。ただし拡張子のキー(Windows の `.juizapp`)は、既定値が juiz のもの(`JUIZ.App`)のときだけ消す(ほかのアプリの関連付けを消さないため、#395)。
- 書き直すときは、前の版が書いた余分なキーが残らないように、juiz のキー(Windows の `JUIZ.App`)を消してから書く。
- ダブルクリックの動作は `juiz run`。インストールは右クリック(Windows・Linux)のメニューに置く。

### 17.2 juiz の実行ファイル

関連付けに書く「juiz の実行ファイル」は次で求める。

- `cargo install` の juiz: `std::env::current_exe()`。
- pip の `juiz-cli`: Windows の `Scripts\juiz.exe` は pip のランチャで、Rust 側の `current_exe()` は python.exe を指す。そのため Python の入口(`juiz_cli:main`)が `sys.argv[0]` から求めたランチャの絶対パスを環境変数 `JUIZ_LAUNCHER` で Rust 側に渡し、あればそれを使う。macOS・Linux も同じ(`bin/juiz` のスクリプト)。
  - pip のランチャは `sys.argv[0]` から `.exe` を落とすので、Windows では `<パス>.exe` があればそれにする。
  - `sys.argv[0]` が実行ファイルでない(`python -m juiz_cli` の `.py` など)ときは入れない(親から受け継いだ値があればそのまま)。求められたときは、親から受け継いだ値より自分のランチャを優先する。

### 17.3 `--launched`

関連付けから起動したことを示す印。`juiz run` と `juiz install` に付ける。付いたときだけ次をする(どの OS も同じ)。

- 起動時に `juiz - <name> <version>` を窓のタイトルにする(Windows は `SetConsoleTitleW`、他は端末のエスケープ `ESC ] 0 ; … BEL`)。
- 起動時に `Close this window or press Ctrl+C to stop.` と 1 行出す。
- **エラーで終わったときは `Press Enter to close.` と出して待つ**(エラーの文が窓ごと一瞬で消えないように)。正常終了・Ctrl+C での停止・§15.1 で「やめる」を選んだときは待たない。標準入力が端末でなければ待たない。
- `juiz install --launched` は、成功したときも `Press Enter to close.` と出して待つ(インストールはすぐ終わるので、待たないと結果を読む前に窓が消える、#395)。
- §15.1 の対話は、`--launched` でも普通に出す。

### 17.4 OS ごとに書くもの

**Windows**(HKCU、#395)

```
HKCU\Software\Classes\.juizapp                       (既定) = JUIZ.App
HKCU\Software\Classes\JUIZ.App                       (既定) = JUIZ application
HKCU\Software\Classes\JUIZ.App\DefaultIcon           (既定) = <アイコン>
HKCU\Software\Classes\JUIZ.App\shell\open\command    (既定) = "<juiz>" run --launched "%1"
HKCU\Software\Classes\JUIZ.App\shell\install         (既定) = Install with juiz
HKCU\Software\Classes\JUIZ.App\shell\install\command (既定) = "<juiz>" install --launched "%1"
```

- 書いた後に `SHChangeNotify(SHCNE_ASSOCCHANGED)` でエクスプローラーに知らせる。
- `<アイコン>` は `<データ>/juizapp.ico` に書き出したもの(pip のランチャにはアイコンを埋められないため)。用意できるまでは `DefaultIcon` を書かなくてよい。
- コンソール窓の × で閉じたとき(`CTRL_CLOSE_EVENT`)は、`SetConsoleCtrlHandler` のハンドラで停止を始め、後始末(相手の System から外れる処理など、#303)が終わるまで最大 5 秒ハンドラから戻らない(戻ると OS がすぐプロセスを止めるため)。
  - ハンドラは `System::wait_for_signal` が最初に呼ばれたときに入れる。`--launched` に関係なく、juiz を動かすどのコンソールでも効く。ログオフ・シャットダウン(`CTRL_LOGOFF_EVENT`・`CTRL_SHUTDOWN_EVENT`)も同じに扱う。Ctrl+C・Ctrl+Break は今まで通り(#307)。
  - 後始末が終わってプロセスが終われば、5 秒を待たずにそのまま終わる。

**Linux**(freedesktop、#396)

`<データ>` は `$XDG_DATA_HOME`(無いか相対パスなら `~/.local/share`)、`<設定>` は `$XDG_CONFIG_HOME`(同じく `~/.config`)。

- `<データ>/mime/packages/juiz-juizapp.xml`: `*.juizapp` → `application/x-juizapp`(`application/zip` の下位、glob の重み 60)。中身を見て zip と判定されても、下位の型なので拡張子の判定が勝つ。
- `<データ>/applications/juiz.desktop`: `Exec=<juiz> run --launched %f`、`Terminal=true`、`NoDisplay=true`(アプリの一覧には出さない)、`MimeType=application/x-juizapp;`。
- `<データ>/applications/juiz-install.desktop`: 名前 `Install with juiz`、`Exec=<juiz> install --launched %f`、`Terminal=true`。ファイルマネージャの「このアプリで開く」に出すため、アクションではなく別の .desktop にし、`NoDisplay` にしない(GNOME は `NoDisplay` のアプリと .desktop のアクションを「このアプリで開く」に出さないため)。
- `<juiz>` は、空白や `"` など Desktop Entry Specification で囲む必要のある文字があるときだけ `"…"` で囲む(xdg-mime の汎用の処理は引用符を外さずに実行ファイルを探すため、囲まなくてよいときは囲まない)。`%` は `%%` にする。
- `<設定>/mimeapps.list`: `[Default Applications]` に `application/x-juizapp=juiz.desktop;`、`[Added Associations]` に `application/x-juizapp=juiz.desktop;juiz-install.desktop;` を書く(`xdg-mime default` が書くのと同じ場所。xdg-utils が無くても効くように直接書く)。
- 書いた後に `update-mime-database <データ>/mime`・`update-desktop-database <データ>/applications` を呼ぶ。コマンドが無いか失敗したら `warning:` を出して続ける。端末エミュレータが開くかはデスクトップ環境に任せる。
- `--remove` は上の 3 ファイルを消し、mimeapps.list の 2 項目は値が上と同じ(juiz のもの)ときだけ消す。

**macOS**(#396)

- ファイルは argv ではなく Apple Event で渡されるので、`~/Applications/JUIZ Launcher.app` を作る。`osacompile` で AppleScript のアプレットとして作り、`on open` で Terminal に `'<juiz>' run --launched '<パス>'` を打たせる(`do script`。引数は `quoted form of` で囲む)。ファイルなしで開いたときは、使い方と指している juiz をダイアログで出す。
- Info.plist を `plutil -replace` で書き換える: `CFBundleIdentifier` = `io.github.openjuiz.launcher`、`CFBundleDocumentTypes`(型 `io.github.openjuiz.juizapp`・拡張子 `juizapp`、役割 Viewer、`LSHandlerRank` Owner)、`UTExportedTypeDeclarations`(`io.github.openjuiz.juizapp`、`public.data` に従う、拡張子 `juizapp`)。
- Info.plist を書き換えると osacompile の署名が合わなくなるので、`codesign --force --sign -` で署名し直す。その後 `lsregister -f` で Launch Services に登録する。どちらも失敗したら `warning:` を出して続ける(osacompile・plutil の失敗はエラー)。
- アプリの中の `Contents/Resources/juiz-launcher.txt` に指している juiz(`executable=<juiz>`)と作り方の版(`layout=1`)を書く。これがあるものだけを juiz が作ったものとして書き換え・削除する。無い同名のアプリがあれば、`juiz associate` はエラーにし、`--remove` は警告だけで残す。
- 自分の機械で作ったアプレットには隔離属性が付かないので、Gatekeeper には止められない。初めて開いたときに「JUIZ Launcher が Terminal を制御しようとしています」の確認が出るので、許可する。
- インストールのメニューは作らない(Finder の右クリックに足すにはサービスが要るため)。

**SIGHUP**: Windows 以外では、端末を閉じたとき(SIGHUP)も止まる合図に足す(`System::wait_for_signal`、#307 と同じ後始末)。
