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
juiz -f juiz.conf -b

cd webui
npm install
npm run dev
```

表示された URL(既定は http://localhost:5173/)を開きます。開発サーバーは `/api` と `/api-docs` を juiz へ中継するので、CORS の設定は要りません。中継先を変えるときは環境変数 `JUIZ_URL` を使います。

```sh
JUIZ_URL=http://192.168.0.10:8000 npm run dev
```

## juiz から配信して動かす

ビルドした画面を、juiz の `option.http_broker.static_filepaths` で配信できます。画面と API が同じ場所になるので、こちらも CORS は要りません。

```sh
cd webui
npm install
npm run build   # webui/dist にできる
```

系の設定ファイルに `/ui` の行を足して juiz を起動し、http://localhost:8000/ui/ を開きます。パスは juiz を起動したディレクトリ(`-C` で指定したディレクトリ)からの相対です。

```yaml
"option":
  "http_broker":
    "port": 8000
    "start": true
    "static_filepaths":
      "/ui": "./webui/dist"
```

リポジトリの最上位で次のように起動すれば、画面だけを試せます。

```sh
juiz -f webui/juiz_webui.conf -d
```

将来は juiz に同梱して `/ui` で配信する予定です([#117](https://github.com/OpenJUIZ/juiz/issues/117))。

## 画面の見方

- 左のツリー: 系 → 処理・容器(その下に容器処理)・実行文脈・接続・話題・仲介 → 各要素。副系(別プロセスの juiz)は系の下に入れ子で出ます。分類や要素の ▸ / ▾ で畳めます。
- 要素を押すと、右の性質欄にその要素の中身が出ます。

| 種類 | 性質欄に出すもの |
|---|---|
| 処理・容器処理 | 型、識別子、説明、言語、生成器、所属容器、引数(名前・型・既定値・拘束値)、戻値型、入力と出力の接続、対象にしている実行文脈、発行・購読する話題 |
| 容器 | 型、識別子、説明、言語、生成器、引数(既定値と生成時の値)、容器処理 |
| 実行文脈 | 型、識別子、状態、周期、自動開始、対象の処理 |
| 接続 | 押込・引込、送り元、送り先と引数、識別子 |
| 話題 | 発行する処理、購読する処理と引数 |
| 仲介 | 型、識別子 |
| 系 | 接続先、UUID、仲介、要素の数 |

性質欄の中の要素名を押すと、その要素に移ります(ツリーで畳まれていれば開きます)。

拘束値(`p_apply` で与えた値)と戻値型は、juiz の profile にまだ出ていないので「—」「不明」と表示します([#112](https://github.com/OpenJUIZ/juiz/issues/112))。

juiz に接続できないときは、上部の状態が「未接続」になり、ツリーの上に理由を出します。前回取れた内容は薄くして残します。

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
- 副系(別プロセスの juiz)は今は描きません([#116](https://github.com/OpenJUIZ/juiz/issues/116))。

## 開発用のコマンド

| コマンド | 内容 |
|---|---|
| `npm run lint` | ESLint |
| `npm run typecheck` | 型検査 |
| `npm run build` | 型検査とビルド |
| `npm run gen:api` | `src/api/openapi.json` から API の型 `src/api/openapi.gen.ts` を作る |
| `npm run gen:api -- http://localhost:8000` | 動いている juiz から OpenAPI 定義を取り直して型を作る |

CI(`.github/workflows/webui.yml`)は lint・型検査・ビルドと、`openapi.gen.ts` が `openapi.json` と合っているかを確かめます。

## 構成

| パス | 内容 |
|---|---|
| `src/api/client.ts` | juiz の HTTP API を呼ぶクライアント |
| `src/api/openapi.json`, `openapi.gen.ts` | juiz の OpenAPI 定義と、そこから生成した型(パスと引数) |
| `src/api/types.ts` | `profile_full` の中身の型。OpenAPI に schema が無いので画面側で書く |
| `src/model.ts` | 取得した profile を画面用の形に組み直す。要素の鍵と、要素どうしの関係(接続の両端、実行文脈の対象など)を解く |
| `src/useSystemModel.ts` | `system/profile_full` と接続の一覧を取得し、自動更新する |
| `src/labels.ts`, `src/marks.ts` | 値の表示用の言葉(実行中・押込など)と、種類ごとのマーク |
| `src/terms.ts` | 画面の用語。漢字を正とし、英語名を小さく並記する |
| `src/settings.ts` | 設定と localStorage への保存 |
| `src/styles/theme.css` | 配色。種類ごとの色(処理は緑、容器は水色、実行文脈は黄、話題はマゼンタ) |
| `src/styles/fonts.css` | 書体。日本語フォントを選んだらここで差し替える |
| `src/components/` | 上部の帯、ツリー、図、性質欄、設定の画面部品 |
| `src/diagram/` | 中央の図の中身。モデルから図の要素と線を作る `graph.ts`、自動配置 `layout.ts`、位置の保存 `positions.ts`、要素・線・湾曲グリッドの部品と `diagram.css` |

## ライセンス

juiz 本体と同じく MIT OR Apache-2.0 です。
