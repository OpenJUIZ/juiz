# Composition(Composite Process / CompositeContainerProcess)仕様

- 状態: 確定(C0、issue #165、2026-10-25)
- 親 issue: #164。計画と経緯はプロジェクトの計画書 composition-plan.md(2026-10-25 確定)にある
- この文書は C1〜C4(#166〜#169)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と用語

複数の Process を、中身の関数を変えずに **1 つの Process としてまとめる**ことを Composition と呼ぶ。

| 用語 | 意味 |
|---|---|
| Composite | Composition で作った Process の型(Factory)。conf の `composites:` で定義する |
| Composite Process | Container を含まない Composite のインスタンス。外からは普通の Process |
| CompositeContainerProcess | ContainerProcess を含む Composite のインスタンス。CompositeContainer を Container に持つ普通の ContainerProcess |
| CompositeContainer | スロット名から既存の Container インスタンスへの参照を束ねた Container |
| 子 | Composite の式の木に現れる Process / ContainerProcess の型(Factory)。**子のインスタンスは作らない** |
| 根 | 式の木の一番外側の子。Composite の戻り値は根の戻り値 |
| ノード | 式の木の中の 1 つの子の呼び出し |
| スロット | CompositeContainer の中の名前付きの Container 参照 |

例(Rust サンプル `opencv_capture_oneshot.conf` の 3 つの Process を 1 つにまとめた形):

```text
capture_gray_save(cc: CompositeContainer{camera, fs}, path)
  = filesystem_imwrite(cc.fs,
        img = cv_cvt_color(img = video_capture_readandget(cc.camera), code = "BGR2GRAY"),
        filename = path)
```

## 2. 基本方針

1. Composite は **Factory(型)として登録する**。インスタンスは今の `ProcessImpl` そのもので、inlets / outlet / 接続 / memo / EC への bind / p_apply / Topic / ブローカー越しの呼び出しをそのまま使う。新しい Process 実装は作らない。
2. 子の関数は変えない。子の Factory の本体関数を取り出して合成する。子のインスタンスは作らず、システムにも登録しない。
3. 式は**木**に限る。各ノードの戻り値は親の引数 1 つにだけ入る。同じ部分式の共有(DAG)は対象外(§13)。
4. 子の間は `Capsule` で値を渡す(汎用経路)。シリアライズの省略は Composite 専用には作らず、遅延評価(#3、L0〜L3)で行う。
5. 定義は conf から始める。API / CLI(C7)、言語側の書き方は後の Step。

## 3. Composite の定義(conf の `composites:`)

### 3.1 形

`composites:` は conf の最上位のキーで、Composite の定義の配列である。

```yaml
composites:
  - type_name: capture_gray_save          # 必須。新しい Factory の type_name
    description: "1 枚撮ってグレーにして保存する"   # 任意
    containers:                           # 任意。CompositeContainer のスロット: スロット名 → Container の type_name
      camera: video_capture
      fs: filesystem
    body:                                 # 必須。式の木の根のノード
      process: filesystem_imwrite
      container: fs                       # 子が ContainerProcess のときに必須。スロット名
      args:
        img:
          process: cv_cvt_color
          args:
            img: { process: video_capture_readandget, container: camera }
            code: { const: "BGR2GRAY" }
        filename: { expose: path }
```

| キー | 必須 | 内容 |
|---|---|---|
| `type_name` | 必須 | Composite の型名。既にある ProcessFactory / ContainerProcessFactory の `type_name` と重なったらエラー(今の `ProcessFactoryOfSameTypeNameAlreadyExistsError` などと同じ扱い) |
| `description` | 任意 | 説明。省略時は空 |
| `containers` | 任意 | スロットの宣言。スロット名 → Container の `type_name`。スロット名は `validate_name` の規則(空でない、`/` `:` `|` を含まない)に従う |
| `body` | 必須 | 根のノード(§3.2) |

- `containers` が空か省略で、木に ContainerProcess を含まないものは **Composite Process の型**(ProcessFactory)として登録する。
- `containers` が 1 つ以上あるものは **CompositeContainerProcess の型**(ContainerProcessFactory、Container の型は `composite_container`)として登録する。
- 宣言したスロットを木のどこでも使っていなければエラーにする(書き間違いを見つけるため)。

### 3.2 ノードと引数

ノードは子の呼び出しで、次のキーを持つ。

| キー | 必須 | 内容 |
|---|---|---|
| `process` | 必須 | 子の Factory の `type_name`。ProcessFactory か ContainerProcessFactory(子の manifest に `container_type` があれば ContainerProcess として扱う) |
| `container` | 子が ContainerProcess のとき必須 | 使うスロット名。スロットの Container の型と、子の Container の型が一致しなければエラー。子が普通の Process なのに書いたらエラー |
| `args` | 任意 | 子の引数名 → 引数の値(下の 3 種のどれか) |

`args` の各値は、次のどれか 1 つのキーだけを持つオブジェクトでなければならない(0 個や 2 個以上はエラー)。

| 形 | 意味 |
|---|---|
| `{ process: ..., container: ..., args: ... }` | 子の式(ノード)。その戻り値をこの引数に渡す |
| `{ const: <値> }` | 定数で拘束する。Composite の引数としては公開しない。定義時に子の引数の型と照合し(`type_check`)、合わなければエラー |
| `{ expose: <公開名> }` または `{ expose: { name: <公開名>, default: <値>, description: <説明> } }` | Composite の引数として公開する(§4)。`default` と `description` は任意 |

- `args` に**書かなかった子の引数は、元の名前のまま公開する**(`{ expose: <元の名前> }` と同じ)。
- 子の manifest にない引数名を `args` に書いたらエラー。
- 子の Factory は**同じ juiz プロセスに登録された Factory**でなければならない。副系(subsystem)やブローカー越しの Factory は使えない(本体関数を取り出せないため)。
- 子に Container を含まない Composite を使ってよい(入れ子)。この場合、子の Composite は他の子と同じく 1 つの Factory として扱う。
- 子に CompositeContainerProcess の型(スロットを持つ Composite)を使うことは、最初の段階ではエラーにする(スロットの受け渡しの規則が要るため。§13)。
- `composites:` の中では、**先に定義した Composite だけ**を子に使える。後ろの定義や自分自身を参照したらエラー(循環を作れないようにするため)。

### 3.3 conf の読み込み順

1. `plugins:`(子の Factory の読み込み)
2. `composites:`(配列の順に 1 つずつ Factory を登録)
3. `processes:`、`containers:`(今のまま)
4. `composite_containers:`(§7。参照先の Container が出来てから作る)
5. 以降は今のまま(brokers、subsystems、ecs、connections)

## 4. 引数を公開する規則

Composite の `arguments`(公開する引数の一覧)は、次の規則で作る。

1. **元の名前のまま公開する。** `args` に書かなかった引数と、`expose` に元の名前を書いた引数は、子の引数名がそのまま公開名になる。
2. **`expose` で名前を変えられる。** `{ expose: path }` は、子の引数を `path` という名前で公開する。公開名は `validate_name` の規則に従う。
3. **同じ公開名を複数の子に渡してよい。** 1 つの公開引数の値を、その名前を持つすべての子の引数に渡す。ただし、それらの子の引数の型(`ArgumentType`)が全部同じでなければエラーにする。
4. **衝突はエラーにする。** 元の名前のまま公開した結果、別々の子の同名の引数が同じ公開名になり、規則 3 の型の条件を満たさない場合はエラーにする。エラーメッセージには衝突した両方のノードの位置(§6)を書き、`expose` で名前を付けるよう案内する。型が同じなら規則 3 として共有になる。
   - 意図しない共有を避けたいときは `expose` で別の名前を付ける。
5. **型・既定値・説明は子から引き継ぐ。** 公開引数の `type_name` は子の引数の型。`default` と `description` は、`expose` のオブジェクト形で指定があればそれを、なければ**最初に出てきた子**の引数のものを使う。指定した `default` は型と照合し、合わなければエラー。
6. **公開する引数の並び**は、式の木を §5 の評価順(根の引数の宣言順、深さ優先)にたどって、**公開名が最初に出てきた順**とする。
7. ContainerProcess の子の Container(第 1 引数に相当するもの)は引数ではなく、スロットから渡す。公開しない。

## 5. 評価

### 5.1 評価順

Composite を `call` すると、根のノードを次の手順で評価する。

```text
eval(node):
    for arg in 子の manifest の arguments の宣言順:
        process なら  値 = eval(子のノード)
        const なら    値 = 定数
        expose なら   値 = Composite の引数(公開名)の値
    子の manifest で引数を検査する(check_manifest_before_call 相当)
    子の本体関数を呼ぶ(ContainerProcess ならスロットの Container を渡す)
    → 戻り値
```

- 順番は**子の manifest の `arguments` の宣言順**で、**深さ優先**(引数を全部評価してから、その子を呼ぶ)である。conf の `args` に書いた順番ではない(conf のオブジェクトはキーの順番を保たないため)。
- ContainerProcess は冪等とは限らないので、この評価順は仕様として固定し、変えるときは互換性のない変更として扱う。
- どこかのノードでエラーが起きたら、そこで評価をやめてエラーを返す。それより後のノードは呼ばない。**それまでに呼んだノードの副作用は取り消さない。**

### 5.2 Composite の入口

- Composite 自体の `call` は、今の `ProcessImpl::call` と同じく、公開引数の一覧で `check_manifest_before_call` をしてから合成した本体を呼ぶ。
- `invoke` / `execute` / 接続 / p_apply / Topic の `subscribes` は、公開引数(inlets)に対して今のまま働く。

### 5.3 memo

- memo は **Composite 単位**。Composite のインスタンスの `use_memo` が今の Process と同じ意味で働く(outlet に 1 つ)。
- 中間値(根以外のノードの戻り値)は memo しない。子の Factory の `use_memo` の設定は使わない。
- そのため、入力の一部だけ変わっても全ノードを計算し直す。

### 5.4 中間値のトレース

- Composite のインスタンスの manifest に `trace: true`(既定は `false`)を書くと、各ノードを評価するたびに、ノードの位置(§6)と戻り値の要約をログ(`log::info`)に出す。
- 中間値を profile や API から読めるようにはしない。

## 6. エラーの位置情報

子でエラーが起きたとき、Composite は「式の木の中のどのノードで失敗したか」を付けて返す。

- **位置の書き方**: 根を `body` とし、子の引数名を `.` でつないだ道筋。例: `body.img.img` は、根の `img` 引数に入るノードの、さらに `img` 引数に入るノード。
- エラーには少なくとも、Composite の `type_name`、ノードの位置、そのノードの子の `type_name`、元のエラーを含める。
  - 例: `Composite(capture_gray_save) failed at body.img.img (video_capture_readandget): <元のエラー>`
- 新しいエラーの種類 `JuizError::CompositeNodeError { type_name, path, child_type_name, error }` を足す(C2)。元のエラーは `anyhow` の context として残す。
- 定義時のエラー(§3、§4 の規則違反)も同じ位置の書き方で、どのノードの何が悪いかを示す。種類は `JuizError::CompositeManifestInvalidError { type_name, path, message }`(C1 で追加)。`path` は規則違反が見つかったノードの位置で、定義全体の規則(使っていないスロットなど)では空にする。

## 7. CompositeContainer(conf の `composite_containers:`)

### 7.1 インスタンスの作り方

```yaml
containers:
  - { type_name: video_capture, name: cv_camera0 }
  - { type_name: filesystem, name: filesystem0 }
composite_containers:
  - name: cam_fs0
    namespace: robot1                    # 任意。今の Container と同じ規則
    slots:
      camera: cv_camera0                 # 名前(CompositeContainer のネームスペースで解決)
      fs: "core://core/Container/filesystem0::filesystem"   # ID でもよい
    processes:
      - { type_name: capture_gray_save, name: cgs0 }
processes:
  - { type_name: some_pure_composite, name: c0 }   # Container を含まない Composite は普通の Process と同じ場所に書く
```

- `composite_containers:` の各要素は `name`(必須)、`namespace`(任意)、`slots`(必須)、`processes`(任意)を持つ。
- `slots` の値は、Container の名前(文字列)、ID 文字列(`://` を含む)、`{ name, namespace }` のどれか。名前のネームスペースは、ネームスペース仕様の規則(相対名は CompositeContainer のネームスペースから、先頭 `/` は絶対)で解決する。
- 参照先はこの juiz プロセスの Container でなければならない(副系の Container は使えない)。見つからなければエラー。
- `processes` に書いた CompositeContainerProcess を作るとき、その Composite が宣言したスロットがすべて `slots` にあり、参照先の Container の `type_name` が宣言と一致しなければエラーにする。`slots` に余分なスロットがあってもよい(1 つの CompositeContainer に、異なるスロットを使う複数の Composite を載せられる)。
- CompositeContainerProcess は、今の ContainerProcess と同じく CompositeContainer のネームスペースに置く(issue #149 と同じ)。

### 7.2 実体

- CompositeContainer は `ContainerImpl<CompositeSlots>` で、`CompositeSlots` はスロット名 → `ContainerPtr` の対応表。参照先の Container は `Arc`(`ContainerPtr`)で持つ。
- CompositeContainer の `type_name` は固定の `composite_container` とする。

### 7.3 ロック

- CompositeContainerProcess の本体は、CompositeContainer をロックしてスロットの `ContainerPtr` を**複製したら、すぐにロックを放す**。そのあと §5 の順に子を呼ぶ。
- 各子は自分の Container を自分でロックする(今の `bind_container_function` のまま)。
- 同じ Container を複数のスロットや複数のノードで使ってよい。子を順に呼ぶので、デッドロックしない。
- **Composite 全体での排他はしない。** 子の呼び出しの合間に、他の Process が同じ Container を触ることがありうる。

### 7.4 寿命

- CompositeContainer のスロットが参照している Container は、**削除を拒否する**。`container_destroy` はエラー(新しい種類 `JuizError::ContainerInUseError`、参照している CompositeContainer の ID を含める)を返す。先に CompositeContainer を消せば消せる。
- CompositeContainer を削除すると、今の Container と同じく、その CompositeContainerProcess を削除し、スロットの参照を放す。参照先の Container は消さない。
- 子の Factory を外す(プラグインを外す)機能は今はないので、Composite が子の本体関数を持ち続けることは問題にならない。外す機能を作るときに扱いを決める。

## 8. ID と profile

### 8.1 ID

ID の形式は変えない。種類(class)も新設しない。

| もの | ID |
|---|---|
| Composite Process | `<broker_type>://<broker_name>/Process/[<ns>/]<name>::<type_name>` |
| CompositeContainerProcess | `<broker_type>://<broker_name>/ContainerProcess/[<ns>/]<name>::<type_name>` |
| CompositeContainer | `<broker_type>://<broker_name>/Container/[<ns>/]<name>::composite_container` |

### 8.2 manifest と profile

Composite の manifest(`ProcessManifest`)と profile(`ProcessProfile`)は、今の項目を次のように埋める。

| 項目 | 値 |
|---|---|
| `arguments` | §4 で作った公開引数の一覧 |
| `outputs` | 根の子の `outputs` |
| `language` | 子(入れ子の Composite は、その子まで含む)の `language` がすべて同じならその値、混ざっていれば `mixed` |
| `factory` | Composite Process は `composite_process_factory`、CompositeContainerProcess は `composite_container_process_factory` |
| `container_type` | CompositeContainerProcess では `composite_container` |
| `publishes` / `subscribes` | インスタンスの manifest に書いたもの。子の Topic の設定は使わない |

新しい任意の項目 `composition` を足す。古いクライアントは知らない項目を無視すればよい(serde の既定値で `None`)。

```yaml
composition:
  containers:                     # スロット名 → Container の type_name(宣言がなければ空)
    camera: video_capture
    fs: filesystem
  body:                           # 正規化した式の木
    process: filesystem_imwrite
    container: fs
    args:
      img:
        process: cv_cvt_color
        args:
          img: { process: video_capture_readandget, container: camera, args: {} }
          code: { const: "BGR2GRAY" }
      filename: { expose: path }
```

- `body` は**正規化した形**で出す。書かなかった引数も `{ expose: <公開名> }` として現れ、すべてのノードに `args` がある。
- Composite の Factory の profile(型の情報)にも同じ `composition` を出す。
- CompositeContainer の profile(`ContainerProfile`)には、スロット名 → 参照先の Container の ID を `slots` として出す。

表示(CLI・管理画面で木を描く)は C5(#170)で扱う。

## 9. 言語の規則

- 汎用経路(§5)は子の間を `Capsule` で渡すので、同じ juiz プロセスの中なら、**言語の違う子を混ぜてよい**。この場合 `language` は `mixed` になる。
- 段の間のシリアライズの省略は、遅延評価(#3、L0〜L3)で、同じ言語・同じ ABI の子の間だけで効く。Composite 側では何もしない。

## 10. 接続(Connection)との使い分け

| | Composition | 接続(Connection) |
|---|---|---|
| 結合の時期 | 定義時(Factory を作るとき) | 実行時(インスタンス同士) |
| 単位 | 1 つの Process(1 回の call、1 つの memo) | ノードごとに Process、memo、ロック |
| 中間結果 | 見えない(`trace` でログに出す) | 各ノードの出力として見える・他から読める |
| つなぎ替え | 定義し直し | 実行中にいつでも |
| 範囲 | 同じ juiz プロセス内 | ブローカー越し・別プロセス・別言語 |
| 分岐 | 木だけ | 1 つの出力を何本にでも配れる(Topic もある) |
| 部分的な memo | 効かない(毎回全ノードを計算) | 変わっていない段は memo が効く |
| 再利用 | 型として何度でもインスタンス化できる | 構成は conf ごと |

Composition が向くもの:

- 画像処理のパイプラインなど、外から 1 回の呼び出しで済ませたい処理の組
- 「センサを読む → 制御則 → モータに出す」を 1 つの EC に bind して、段の順番を保証したいとき
- 既存の Process の前後に単位変換やアダプタを付けたいとき(どちらの Process も直さない)
- 内部の引数を `const` で隠し、外に出す引数を絞りたいとき

接続が向くもの:

- 中間結果を複数の相手に配りたい、他から読みたい
- 別プロセス・別マシン・副系の Process をつなぎたい
- 実行中につなぎ替えたい
- 重い段の memo を効かせたい

## 11. 互換性

- 変えないもの: `Process` トレイト、`ProcessImpl`、接続の仕組み、通信形式、ID の形式、既存の C ABI 関数、既存の conf のキー。
- 足すもの: conf の `composites:` / `composite_containers:`、`ProcessManifest` / `ProcessProfile` の任意の項目 `composition`、`ContainerProfile` の `slots`、`ProcessFactory::body()`(子の本体関数を取り出す)、エラーの種類 `CompositeManifestInvalidError` / `CompositeNodeError` / `ContainerInUseError`。

## 12. Step との対応

| Step | issue | この文書の範囲 |
|---|---|---|
| C1 SDK | #166 | §3.2 の式の木の型(`CompositeManifest`)、§4 の公開引数を作る関数と衝突検出、§8.2 の `composition`。juiz_sdk の `manifests::composite_manifest`(`CompositeManifest::build_process_manifest`) |
| C2 core(Container なし) | #167 | `ProcessFactory::body()`、§5 の評価、§6 のエラー、§3.2 の入れ子と循環の検出 |
| C3 core(Container あり) | #168 | §7 の CompositeContainer、ロック、寿命 |
| C4 conf とベンチマーク | #169 | §3.1 / §3.3 / §7.1 の conf の読み込み、ネームスペース、ベンチマーク |
| C5 表示 | #170 | §8.2 の `composition` と `slots` の表示 |
| C8 型検査 | #172 | ノードの戻り値の型と親の引数の型の定義時検査(#163 Step 4 の比較関数を使う) |

## 13. 対象外(後の Step か、未定)

- DAG(同じ部分式を 1 回だけ評価して複数の引数に配る)
- CompositeContainerProcess の型を子に使う入れ子(スロットの受け渡し)
- Composite 全体での Container の排他
- 実行時の定義(ブローカー API・CLI、C7 #171)と、言語側での定義(pyjuiz の `compose(...)` など)
- 接続済みのインスタンス群から Composite を作る機能(`from-connections`)
- 式の文字列表記(`filesystem_imwrite(fs, img=cv_cvt_color(...), filename=$path)`)
- Composite から IDL の `process` 宣言を書き出す機能(#163 関連)
- 段の間のシリアライズの省略(遅延評価、#3 の L0〜L3)
