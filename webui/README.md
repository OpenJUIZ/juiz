# juiz 管理画面 (webui)

ブラウザで juiz の系(System)を確認する画面です。親 issue は [#110](https://github.com/OpenJUIZ/juiz/issues/110)。
今は左のツリーと右の性質欄([#114](https://github.com/OpenJUIZ/juiz/issues/114))までできています。中央の図は [#115](https://github.com/OpenJUIZ/juiz/issues/115) で作ります。

TypeScript + React + Vite で作り、図は React Flow(`@xyflow/react`)、自動配置は elkjs を使います。

## 必要なもの

- Node.js 22 以上と npm
- 動いている juiz(HTTP Broker を有効にしたもの)

## 開発サーバーで動かす

```sh
# juiz を HTTP Broker 付きで起動しておく(既定は http://localhost:8000)
juiz -f juiz.conf --http

cd webui
npm install
npm run dev
```

表示された URL(既定は http://localhost:5173/)を開きます。開発サーバーは `/api` と `/api-docs` を juiz へ中継するので、CORS の設定は要りません。中継先を変えるときは環境変数 `JUIZ_URL` を使います。

```sh
JUIZ_URL=http://192.168.0.10:8000 npm run dev
```

## juiz に同梱した画面を開く

juiz は管理画面を埋め込んでいて、HTTP Broker を有効にすると `/ui` で配信します([#117](https://github.com/OpenJUIZ/juiz/issues/117))。起動すると次のように URL が出るので、ブラウザで開きます。

```sh
$ juiz -f juiz.conf -d
juiz 管理画面: http://localhost:8000/ui/
```

画面と API が同じ場所になるので、CORS の設定は要りません。

- crates.io の juiz_app(`cargo install juiz_app`)と pip の juiz-cli には、ビルド済みの画面が入っています。Node.js は要りません。
- リポジトリから juiz をビルドするときは、先に `cd webui && npm ci && npm run build` で `webui/dist` を作っておくと、juiz_webui クレート(`juiz_webui/`)の `build.rs` がそれを埋め込みます(画面をビルドし直したら `cargo build` し直す)。`webui/dist` が無いままビルドした juiz では、`/ui/` は画面のビルド方法の案内になります。
- 画面のビルド成果物は juiz_webui クレートに入り、juiz_core / juiz_app の feature `webui`(既定でオン)で使います。外すときは `--no-default-features` で他の feature を選び直します。
- 埋め込む元のディレクトリは環境変数 `JUIZ_WEBUI_DIR` で指定することもできます。

配信を止めるときは設定ファイルで `webui: false` にします。

```yaml
"option":
  "http_broker":
    "port": 8000
    "start": true
    "webui": false
```

### ビルドしたディレクトリを配信する

埋め込みの代わりに、ビルドした画面を `option.http_broker.static_filepaths` で配信することもできます。`/ui` を指定すると、埋め込みの画面ではなくこちらを配信します。パスは conf のあるディレクトリからの相対です(0.0.29 まではカレントディレクトリからの相対でした。conf からの相対で見つからずカレントディレクトリからは見つかるときは、警告を出してそちらを使います)。

```yaml
"option":
  "http_broker":
    "port": 8000
    "start": true
    "static_filepaths":
      "/ui": "./dist"   # webui/juiz_webui.conf から見たパス
```

リポジトリの最上位で次のように起動すれば、juiz をビルドし直さずに `webui/dist` の画面を試せます。

```sh
cd webui && npm run build && cd ..
juiz -f webui/juiz_webui.conf -d
```

## 画面の見方

- 左のツリー: 系 → 処理・容器(その下に容器処理)・実行文脈・接続・話題・仲介 → 各要素。副系(別プロセスの juiz)は系の「副系」の下に入れ子で出し、経由する仲介(http、qmp など)を併記します。中身を取れなかった副系には「取得不可」と出します。分類や要素の ▸ / ▾ で畳めます。
- 接続は、出す側の処理がいる系の下に出ます。別の系どうしの接続も 1 本にまとめます。
- 要素を押すと、右の性質欄にその要素の中身が出ます。

| 種類 | 性質欄に出すもの |
|---|---|
| 処理・容器処理 | 型、識別子、説明、言語、生成器、所属容器、引数(名前・型・既定値・拘束値)、戻値型、呼び出すフォーム、入力と出力の接続、対象にしている実行文脈、発行・購読する話題 |
| 容器 | 型、識別子、説明、言語、生成器、引数(既定値と生成時の値)、容器処理 |
| 実行文脈 | 型、識別子、状態、周期、自動開始、対象の処理 |
| 接続 | 押込・引込、送り元、送り先と引数、識別子 |
| 話題 | 発行する処理、購読する処理と引数 |
| 仲介 | 型、識別子 |
| 系 | 接続先、経由する仲介(副系)、取得できない理由、UUID、仲介、要素の数 |

性質欄の中の要素名を押すと、その要素に移ります(ツリーで畳まれていれば開きます)。

型付きの引数(データ型、[#197](https://github.com/OpenJUIZ/juiz/issues/197))は、型の欄に型の名前(`Pose2D` など)を出します。名前にマウスを載せると完全名と指紋が出ます。

処理・容器処理の「呼び出す」では、引数を入力して処理を呼び、結果を見られます。型付きの引数は型から入力欄を作ります(record は入れ子の欄、enum は選択、`T?` は null のチェック、配列と map は要素の追加と削除)。型なしの引数は型名(Int・Float・Bool・String は欄、Array・Object は JSON)から作ります。「JSON で編集」で引数全体を JSON で書くこともできます。画像の引数はこの画面からは渡せません。

拘束値(`p_apply` で与えた値)と戻値型は、juiz の profile にまだ出ていないので「—」「不明」と表示します([#112](https://github.com/OpenJUIZ/juiz/issues/112))。

juiz に接続できないときは、上部の状態が「未接続」になり、ツリーの上に理由を出します。前回取れた内容は薄くして残します。

## 別の場所から開くとき(CORS)

画面を juiz とは別のオリジン(別のホストやポート)から開くときは、juiz 側で CORS を許可します。`option.http_broker.cors_allow_origins` に許可するオリジンを並べます(既定では許可しません)。`"*"` と書けばどこからでも呼べるようになるので、手元で試すときだけにしてください。

```yaml
"option":
  "http_broker":
    "port": 8000
    "start": true
    "cors_allow_origins":
      - "http://localhost:5173"
```

## 設定

左のツリーの上にある「設定」ボタンから変えます。値はブラウザの localStorage に保存され、juiz の host:port ごとに別になります。

| 項目 | 内容 |
|---|---|
| 接続先 | 空なら画面と同じ場所。別の場所の juiz を指すと、juiz 側に CORS の設定が要ります([#112](https://github.com/OpenJUIZ/juiz/issues/112)) |
| 更新間隔(秒) | 自動更新の間隔。0 以下で自動更新なし(上部の「更新」ボタンだけ)。最短 0.5 秒 |

## 中央の図

系の要素を [#111](https://github.com/OpenJUIZ/juiz/issues/111) で決めた表記で描きます。

| 要素 | 図 |
|---|---|
| 処理 | 緑の楕円 |
| 容器 | 水色の長方形。容器処理(緑の楕円)は容器の外に置き、容器から水色の線で結ぶ(処理側の端が白丸) |
| 実行文脈 | 黄色の長方形。停止中 / 実行中の枠のうち今の状態が光り、「実行中」の枠から対象の処理へ黄色の矢印 |
| 話題 | マゼンタの長方形。処理 → 話題が赤の矢印(発行)、話題 → 処理がマゼンタの矢印(購読) |
| 接続 | 処理 → 処理の矢印。押込は実線、引込は破線。矢印の先に引数名 |

- 配置は elkjs で左から右へ自動で決めます。要素の顔ぶれや線が変わったときだけ配置し直します。
- 要素はドラッグで動かせ、位置はブラウザの localStorage に接続先ごとに保存されます。図の右上の「自動配置」で保存した位置を消して自動配置に戻し、「全体」で全体が見える倍率にします。
- 要素や接続の線をクリックすると選ばれ、ツリーと性質欄にも同じものが出ます。選んだ要素には照準の輪が重なり、処理なら輪の左に引数ごと、右に出力の弧が出ます(つながっているものが光る)。
- 副系(別プロセスの juiz)があるときは、系ごとに破線の枠で囲み、別の系どうしの接続は枠をまたぐ矢印で描きます([#116](https://github.com/OpenJUIZ/juiz/issues/116))。枠は中の要素を囲む大きさになり、見出しを押すとその系が選ばれます。

## 開発用のコマンド

| コマンド | 内容 |
|---|---|
| `npm run lint` | ESLint |
| `npm run typecheck` | 型検査 |
| `npm run build` | 型検査とビルド |
| `npm run gen:api` | `src/api/openapi.json` から API の型 `src/api/openapi.gen.ts` を作る |
| `npm run check:fonts` | 画面の字がすべて同梱フォントに入っているか確かめる |
| `npm run fonts:chars` | 同梱フォントに入れる字の一覧 `src/assets/fonts/subset-chars.txt` を作り直す |
| `npm run gen:api -- http://localhost:8000` | 動いている juiz から OpenAPI 定義を取り直して型を作る(プラグインを読み込んでいない juiz から取る。OpenAPI 定義には読み込んだ Process の引数と型のスキーマが入るため) |

CI(`.github/workflows/webui.yml`)は lint・型検査・ビルドと、`openapi.gen.ts` が `openapi.json` と合っているか、画面の字が同梱フォントに入っているかを確かめます。

### 同梱フォント

漢字・かなは [Zen Old Mincho](https://github.com/googlefonts/zen-oldmincho)(SIL Open Font License 1.1)を、画面で使う字だけに絞った woff2 にして同梱しています([#111](https://github.com/OpenJUIZ/juiz/issues/111))。図と見出しは Black、性質欄などの小さい文字は Regular です。外部 CDN は使いません。

画面に新しい字(漢字など)を足して `npm run check:fonts` が落ちたら、字の一覧と woff2 を作り直します。元の ttf は [google/fonts の ofl/zenoldmincho](https://github.com/google/fonts/tree/main/ofl/zenoldmincho) から取ります(1 本 5MB あるのでリポジトリには入れません)。

```sh
pip install fonttools brotli
npm run fonts:chars
scripts/subset-fonts.sh <ZenOldMincho-Regular.ttf と ZenOldMincho-Black.ttf を置いたディレクトリ>
```

一覧に無い字(系から届く名前など)は、ブラウザの明朝体(ヒラギノ明朝、游明朝など)で出ます。

## 構成

| パス | 内容 |
|---|---|
| `src/api/client.ts` | juiz の HTTP API を呼ぶクライアント |
| `src/api/openapi.json`, `openapi.gen.ts` | juiz の OpenAPI 定義と、そこから生成した型(パスと引数) |
| `src/api/types.ts` | `profile_full` の中身の型。OpenAPI に schema が無いので画面側で書く |
| `src/avro.ts` | 型参照の Avro のスキーマを入力フォーム用の形に読む |
| `src/model.ts` | 取得した profile を画面用の形に組み直す。要素の鍵と、要素どうしの関係(接続の両端、実行文脈の対象など)を解く |
| `src/useSystemModel.ts` | `system/profile_full?recursive=true`(副系の中身と各系の接続つき)を取得し、自動更新する。古い juiz なら接続を一つずつ取る |
| `src/labels.ts`, `src/marks.ts` | 値の表示用の言葉(実行中・押込など)と、種類ごとのマーク |
| `src/terms.ts` | 画面の用語。漢字を正とし、英語名を小さく並記する |
| `src/settings.ts` | 設定と localStorage への保存 |
| `src/styles/theme.css` | 配色。種類ごとの色(処理は緑、容器は水色、実行文脈は黄、話題はマゼンタ) |
| `src/styles/fonts.css` | 書体。同梱フォントの `@font-face` と、見出しの太さ `--font-weight-heading` |
| `src/assets/fonts/` | 同梱フォント(画面の字だけに絞った woff2)と字の一覧 `subset-chars.txt` |
| `public/fonts/ZenOldMincho-OFL.txt` | 同梱フォントのライセンス。ビルドした画面の `fonts/` に入る |
| `src/components/` | 上部の帯、ツリー、図、性質欄(呼び出すフォーム `CallForm.tsx` を含む)、設定の画面部品 |
| `src/diagram/` | 中央の図の中身。モデルから図の要素と線を作る `graph.ts`、自動配置 `layout.ts`、位置の保存 `positions.ts`、要素・線・湾曲グリッドの部品と `diagram.css` |

## ライセンス

juiz 本体と同じく MIT OR Apache-2.0 です。ただし同梱フォント `src/assets/fonts/ZenOldMincho-*.woff2` は Zen Old Mincho Project Authors の著作物で、SIL Open Font License 1.1([`public/fonts/ZenOldMincho-OFL.txt`](public/fonts/ZenOldMincho-OFL.txt))で配布します。Reserved Font Name は定められていないので、絞ったものも名前は変えていません。
