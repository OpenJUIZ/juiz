# ContainerModel(Container の 3D モデル)仕様

- 状態: 確定(Step 0、issue #380、2026-11-14)
- 親 issue: #379。計画と経緯はプロジェクトの計画書 model-plan.md(2026-11-02 確定)にある
- この文書は Step 1〜7(#381〜#386、site#65)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す。「(Step n で決める)」と書いた所は、その Step の PR でこの文書に書き足す
- 位置姿勢・Transform・Anchor の言葉と計算規則は [transformations.md](transformations.md) に従う

## 1. 目的と用語

Container に 3D モデルのファイル(形)を持たせ、外から取り出せるようにする。Transformations(ContainerPose・Transform・Anchor)と組み合わせると、「どの Container が、どこに、どんな形で」あるかがそろい、システム全体を 3 次元で表示できる。

| 用語 | 意味 |
|---|---|
| **ContainerModel** | Container が持つ 3D モデル。ファイルの中身(バイト列)と、その付帯情報の組。ContainerPose にそろえた名前。conf のキーは `model`、API は `container/model` |
| **モデルのファイル** | ContainerModel の中身になる 1 つのファイル(.glb・.stl・.sdf など) |
| **モデル情報**(model info) | 中身を除いた付帯情報(形式・大きさ・指紋・縮尺・取り付け位置・上の軸・色)。小さな JSON で、型は `juiz_std.model.ContainerModelInfo`(§8)。`container/profile_full` に載る |
| **取り付け位置**(origin) | Container の座標から見たモデルの位置姿勢。モデルの原点が Container の原点とずれているときに使う。値の形は `juiz_std.geometry.Transform` |
| **形式の変換** | モデルのファイルの「上の軸」を juiz の Z 上にそろえる回転(§4.2) |

## 2. 基本方針

1. **juiz(core)はモデルの中身を解釈しない**。core はモデルのファイルを不透明なバイト列として扱い、読むのは形式の判定に要る先頭(と SDF の最上位の要素)・大きさ・SHA-256 だけ。メッシュは開かない。表示や計算は受け取った側(管理画面の 3D 表示、利用者のツール)がする。形式ごとの読み込み器を core に入れると依存と保守が膨らむため。
2. **1 Container に 1 つ**。部品が多いロボットは部品ごとに Container を分け、Transform でつなぐ(関節で動く部分はもともと分かれている)。後から複数に広げるときは、今の `model:` を「1 つだけの列」とみなして互換を保つ。
3. **モデルは Container の属性**であり、Process(ContainerProcess)にはしない。Process にすると list・図・接続の対象に紛れ込み、利用者の ContainerProcess の名前ともぶつかる。取り出しは CRUD の API(§6)で行う。取り出したい Process は利用者が API を使って自分で書ける。
4. **conf に書く機能**なので、Container の中身の言語(Rust・Python・C++・Haskell)に関係なく使える。SDK の言語ごとの変更は要らない。
5. ファイルは**作成時に確かめ、中身はメモリに持たない**。取り出しのたびにファイルから読む(§5.3)。

## 3. 形式

### 3.1 形式と判定

| `format` | 拡張子 | 先頭の確かめ | Content-Type(`media_type`) | 既定の `up_axis` | 管理画面の 3D 表示 |
|---|---|---|---|---|---|
| `glb` | `.glb` | 先頭 4 バイトが `glTF`(0x67 0x6C 0x54 0x46)で、続く 4 バイト(リトルエンディアンの u32)が `2` | `model/gltf-binary` | `y` | する |
| `gltf` | `.gltf` | JSON のオブジェクトとして読め、`asset.version` が `2` で始まる文字列 | `model/gltf+json` | `y` | する(データ埋め込みのものだけ、§3.3) |
| `stl` | `.stl` | バイナリ: 大きさが `84 + 50 × n`(n は 80 バイト目からのリトルエンディアンの u32)。それ以外で先頭が `solid` ならアスキー | `model/stl` | `z` | する |
| `sdf` | `.sdf` | XML として読め、最上位の要素が `<sdf>` で、その直下に `<model>` がちょうど 1 つ(§3.2) | `application/xml` | `z` | する(§3.2 の部分集合) |
| `other` | 上のどれでもない | 確かめない | `application/octet-stream` | `z` | しない(取り出しだけ) |

- 形式は拡張子(大文字小文字は区別しない)で決め、表の「先頭の確かめ」で軽く確かめる。合わなければ Container の作成をエラーにする(`ContainerModelInvalidError`、§11)。
- conf の `format:` で明示してよい(拡張子が無い・違うファイルのため)。明示したときは拡張子を見ないが、先頭の確かめはする。`format: other` は何も確かめない。
- `.obj`・`.dae`・`.ply` などは `other` として置いて取り出すことはできるが、管理画面では表示しない(名前と座標軸だけ出す)。
- 先頭の確かめで読むのは、glb・バイナリ STL は先頭 84 バイトまで、gltf・sdf はファイル全体(どちらも手書きか小さなテキストの想定。§5.2 の上限は効く)。

### 3.2 SDF の部分集合(手書き用)

SDF(SDFormat、Gazebo の XML)を、テキストで簡単に書ける形式として受け付ける。単位はメートル・Z 上で juiz の約束(§4)とそのまま合う。Container 1 つのモデルとして読むのは次の部分集合だけ。

```xml
<sdf version="1.9">
  <model name="pantilt">
    <link name="body">
      <visual name="base">
        <pose>0 0 0.02 0 0 0</pose>            <!-- x y z roll pitch yaw (m, rad) -->
        <geometry><cylinder><radius>0.04</radius><length>0.04</length></cylinder></geometry>
        <material><diffuse>0.2 0.2 0.2 1</diffuse></material>
      </visual>
      <visual name="head">
        <pose>0 0 0.07 0 0 0</pose>
        <geometry><box><size>0.06 0.04 0.04</size></box></geometry>
      </visual>
    </link>
  </model>
</sdf>
```

読むもの:

| 要素 | 意味 |
|---|---|
| `<model>` の `<pose>` | Container の座標から見た model の位置姿勢(省略時は単位)。origin(§4.3)の後に掛ける |
| `<link>` の `<pose>` | model から見た link の位置姿勢(省略時は単位) |
| `<visual>` の `<pose>` | link から見た visual の位置姿勢(省略時は単位) |
| `<visual>` の `<geometry>` | `<box><size>x y z</size>`、`<cylinder><radius>`・`<length>`(Z 軸向き)、`<sphere><radius>`、`<capsule><radius>`・`<length>`(Z 軸向き、length は円柱部分)、`<ellipsoid><radii>x y z</radii>`、`<plane><normal>`・`<size>`(normal の既定は `0 0 1`)、`<mesh><uri>`・`<scale>`(§3.2.1) |
| `<visual>` の `<material>` | `<diffuse>` の RGBA(0〜1)。無ければ `<ambient>`。どちらも無ければ conf の `color`、それも無ければ表示側の既定の色 |

- `<pose>` は「x y z roll pitch yaw」の 6 つの数(m、rad)。回転は固定軸の roll(x)→ pitch(y)→ yaw(z)の順、つまり R = Rz(yaw)·Ry(pitch)·Rx(roll)(SDF と ROS の RPY と同じ)。属性 `degrees="true"` があれば角度は度。`relative_to` 属性は、空か「すぐ上の要素(visual なら自分の link、link なら model)」だけを受け付け、それ以外は警告して無視する。
- `<collision>`・`<inertial>`・`<sensor>`・`<plugin>`・`<light>`・`<frame>`・`<include>`・`<static>` などは読み飛ばす。
- `<link>` が複数あれば、各 link をその `<pose>` の位置に**固定で**置く(全体を 1 つの剛体として描く)。`<joint>` があれば「動く部分は Container を分けるか、取り込み(Step 9 #388)を使う」と警告する。
- `<world>` を最上位に置いたもの、`<model>` が 2 つ以上、`<model>` の入れ子は、この部分集合に入らない。core は `<sdf>` の直下の `<model>` が 1 つでないときに作成をエラーにする。入れ子の `<model>` は警告して読み飛ばす。

#### 3.2.1 SDF の `<mesh>`

- `<mesh><uri>` は .sdf からの相対パスの .glb / .stl だけを想定する。`model://`・`package://`・URL は対応しない(取り込み Step 9 で扱う)。
- 今の API(§6)は 1 Container につき 1 ファイルしか配らないので、`<mesh>` の指す先は管理画面から取り出せない。**`<mesh>` は書いてよいが、最初の版では表示しない**(表示側は大きさの分からない箱ではなく座標軸だけ出す)。`juiz model check`(§9.2)は `<mesh>` を見つけたら警告する。複数ファイルの配信は将来(§13)。

#### 3.2.2 SDF を解釈する所

SDF の要素を形に直すのは**表示する側**(管理画面が XML を読んで three.js の基本形にする)。core は §3.1 の表のとおり「XML として読め、`<sdf>` の直下に `<model>` が 1 つ」だけ確かめる(§2 の 1)。書き間違いの細かい検査(数の個数、未知の geometry、joint、mesh、relative_to)は CLI の `juiz model check`(§9.2)で教える。

### 3.3 glTF の注意

- `.gltf` で `buffers[].uri`・`images[].uri` が `data:` で始まらない(外部ファイルを参照する)ものは、1 ファイルで完結しない。core は作成時に**警告**を出し(エラーにはしない)、取り出しはそのファイルだけ返す。管理画面は外部ファイルを取りに行かない(表示できないものは座標軸だけ)。
- .glb の中の外部参照(`uri` を持つ buffer・image)も同じ扱い。ただし core は .glb の JSON チャンクを読まない(§2 の 1)ので、警告は `juiz model check` だけが出す。
- glTF の拡張(Draco 圧縮・KTX2 など)は、表示側の読み込み器が対応していれば表示できる。core は関知しない。

### 3.4 URDF

URDF は Container のモデルではなく「複数の Container と Transform とモデルの組」の記述なので、`model:` には書けない(拡張子 `.urdf` は `other` になるが、作成時に「URDF は取り込み(Step 9 #388)を使う」と警告する)。将来「URDF・複数 link の SDF を読んで conf(containers / transforms / model)を作る」取り込みを別 Step にする。

## 4. 単位と座標

### 4.1 juiz の約束

**メートル・右手系・Z 上**(ROS の REP 103 と同じ。x 前・y 左・z 上)。Transformations の位置姿勢と同じ約束([transformations.md](transformations.md) §3.1)。角度はラジアン、四元数は (x, y, z, w)。

### 4.2 形式の変換(上の軸)

`up_axis` はモデルのファイルの中の「上」の軸。`y` か `z`。

| `up_axis` | 形式の変換 U | 四元数 (x, y, z, w) |
|---|---|---|
| `z` | 単位(回さない) | (0, 0, 0, 1) |
| `y` | x 軸まわり +90°(ファイルの +Y を juiz の +Z に、ファイルの +Z を juiz の −Y に) | (0.7071067811865476, 0, 0, 0.7071067811865476) |

- 既定は §3.1 の表のとおり、glTF(glb・gltf)は `y`(glTF の仕様が Y 上のため)、それ以外は `z`。Z 上で書き出した glTF は `up_axis: z` と書けば回さない。
- 変換は「上」だけをそろえ、「前」はそろえない。x 軸まわりの +90° は、Z 上の道具(Blender など)から glTF を書き出すときの変換のちょうど逆なので、その道具で見た向きのまま置かれる。glTF の仕様の「前は +Z」に合わせたいときは、`origin` で z 軸まわりに回す。

### 4.3 縮尺

- `scale` はモデルのファイルの 1 単位を何メートルにするかの倍率(正の有限の数、3 軸とも同じ)。既定は `1.0`。
- STL には単位が無い。CAD から mm で出したものは `scale: 0.001` と書く。glTF の単位は仕様上メートルなので、ふつうは書かない。
- 0 以下・NaN・無限大はエラー(`ContainerModelInvalidError`)。

### 4.4 表示位置

モデルのファイルの中の点 p_m を、Anchor A から見た点 p_A に写す式:

```text
pose_A(モデル) = pose_A(C) ∘ origin ∘ U ∘ S
p_A            = pose_A(C) · ( origin · ( U · ( scale × p_m ) ) )
```

- `pose_A(C)` は Transformations の計算規則(transformations.md §3.4)の値。`∘` と `·` も transformations.md §3.3 と同じ。
- `origin` は Container の座標で書く取り付け位置(既定は単位)。U は §4.2、S は §4.3 の一様な拡大。
- SDF では、さらに `U` の右に model・link・visual の `<pose>` が掛かる(SDF は `z` なので U は単位)。

数値の例(管理画面と `juiz model check` のテストで同じ値を確かめる):

- Container C が `pose_A(C)` = position (1, 0, 0)、orientation 回転なし。`model: {path: x.glb, origin: {translation: {x: 0, y: 0, z: 0.5}}}`(`up_axis` は既定の `y`、`scale` は 1)。ファイルの点 p_m = (0, 1, 0)(glTF の「上」1 m)は p_A = (1, 0, 1.5) に、p_m = (0, 0, 1)(glTF の +Z 1 m)は p_A = (1, −1, 0.5) に置かれる。
- 同じ C で `model: {path: x.stl, scale: 0.001}`。p_m = (100, 0, 0)(100 mm)は p_A = (1.1, 0, 0) に置かれる。

比べるときの許容誤差は 1e-9。

## 5. conf

### 5.1 書き方

```yaml
containers:
  - type_name: pantilt
    name: pantilt0
    model:
      path: models/pantilt.glb          # conf からの相対
  - type_name: camera
    name: camera0
    model:
      path: models/camera.stl
      scale: 0.001                      # mm → m
      origin:                           # Container の座標から見たモデルの位置
        translation: {x: 0, y: 0, z: -0.02}
        rotation: {x: 0, y: 0, z: 0, w: 1}
      color: "#404040"                  # 色を持たない形式の表示色
  - type_name: static_anchor
    name: floor
    model:
      path: models/floor.sdf            # Anchor もモデルを持てる
```

| キー | 型 | 既定 | 意味 |
|---|---|---|---|
| `path` | 文字列 | (必須) | モデルのファイルのパス。相対パスは **conf の親ディレクトリからの相対**(プラグイン・ブローカー・静的ファイルと同じ、app-package.md §5.3・§5.4)。絶対パスも書ける |
| `format` | `glb` / `gltf` / `stl` / `sdf` / `other` | 拡張子から(§3.1) | 形式の明示 |
| `scale` | 数 | `1.0` | §4.3 |
| `origin` | `juiz_std.geometry.Transform` の辞書 | 単位 | §4.4。`translation` / `rotation` は省略時それぞれ 0 と回転なし(transformations.md §11 の `transforms:` と同じ) |
| `up_axis` | `y` / `z` | 形式から(§3.1) | §4.2 |
| `color` | `"#RRGGBB"` か `"#RRGGBBAA"` | なし | 色を持たない形式・要素(STL、SDF で `<material>` の無い visual)の表示色。色を持つ形式(glTF)では使わない |

- `model` は Container ごとに 1 つ。Anchor(`periodic_anchor` / `static_anchor`)・CompositeContainer も Container なので持てる。
- 表に無いキーは警告を出して使わない(Process の知らないキーの警告 #401 と同じ扱い)。
- `origin` の四元数は保存するときに正規化する。長さ 0・NaN・無限大はエラー(transformations.md §3.1 の規則、エラーは `ContainerModelInvalidError`)。
- ネームスペースで複製したロボットが同じモデルのファイルを指してよい。

### 5.2 作成時の検査

Container の作成時(conf の読み込み)に次を順に確かめ、満たさなければ **Container の作成をエラーにする**(他の conf のエラーと同じく ERROR を出す。起動を止めるかは他の conf のエラーと同じ扱い)。

1. `model` の各キーが §5.1 の型に合う(`ContainerModelInvalidError`)。
2. `path` を解決したファイルがあり、普通のファイルとして開ける(`ContainerModelFileNotFoundError`)。シンボリックリンクは辿ってよい。
3. 大きさが上限(§5.4)以下(`ContainerModelInvalidError`)。
4. 形式を決め、§3.1 の先頭の確かめに合う(`ContainerModelInvalidError`)。
5. SHA-256 を計算して覚える。

覚えるのは「解決した絶対パス・形式・大きさ・更新時刻・SHA-256」と conf の値だけで、中身は持たない。

### 5.3 取り出すとき

- 取り出しの要求(§6 の `container/model_info`・`container/model`・`container/profile_full`)のたびにファイルの大きさと更新時刻を見る。どちらかが変わっていれば §5.2 の 3〜5 をやり直して覚え直す(SHA-256 も ETag も変わる)。変わっていなければ SHA-256 は計算し直さない。
- `container/model` は、覚え直した後にファイルを読み、読んだバイト列の大きさが覚えた値と違えば(読む間に書き換わった)、もう一度だけ覚え直して読み直す。
- 実行中にファイルが消えた・上限を超えた・形式が合わなくなったときは、その要求をエラーにし(`ContainerModelFileNotFoundError` / `ContainerModelInvalidError`)、WARN を出す。Container は消さない。ファイルが戻れば次の要求から元に戻る。

### 5.4 大きさの上限

- 既定は 64 MiB(67,108,864 バイト)。conf の `option.model.max_bytes` で変えられる(System 全体で 1 つ)。
- JSON の経路(§6.3)は base64 で 4/3 倍になるので、16 MiB を超えるモデルを JSON の経路で返すときは WARN を出す(1 Container につき 1 回)。

### 5.5 API からの作成

`model` は **conf でだけ書ける**。ブローカー越しの `container/create`(HTTP・qmp・IPC・メッセンジャー、pyjuiz の `SystemProxy`)の manifest に `model` があれば `ContainerModelInvalidError` にする。理由: 遠くの利用者が任意のパスを指して、juiz を動かす機械の上のファイルを読み出せてしまうため。API での作成・差し替えは将来(§13)。

Step 1 で決めたこと: 断るのは `container/create` の `__manifest__` の中と外(引数の `model`)、`composite_container/create`、Rust の `CoreBroker::container_create` に `ContainerManifest::model` を付けたとき。どれも Container を作る前に断る。

## 6. API

### 6.1 クラス / 関数

| 種類 | クラス / 関数 | 引数 | 戻り値 |
|---|---|---|---|
| read | `container/profile_full` | `identifier` | 今の profile に `model` を足す。値はモデル情報(§8)、モデルが無ければ `null` |
| read | `container/model_info` | `identifier` | モデル情報(§8)。モデルが無ければ `null` |
| read | `container/model` | `identifier` | モデルの中身(§6.2・§6.3)。モデルが無ければ `ContainerHasNoModelError` |

- `identifier` は Container の ID か名前(transformations.md §13.1 の Step 3 で決めた解決の規則と同じ、根のネームスペースから)。**パスは受け付けない**(§5.5 と同じ理由)。
- HTTP では READ(GET)、`identifier` はクエリ。パスは `/api/container/model_info`・`/api/container/model`。
- サブシステム(別の System)の Container も、BrokerProxy 越しに取り出せる(管理画面が親の System だけにつないでいても全部見える)。中継する System はモデルを持たず、そのまま渡す。

### 6.2 HTTP の `container/model`(生のバイト列)

Step 1 で決めたこと: `If-None-Match` があれば、中身を読む前にモデル情報の SHA-256 と比べ、同じなら読まずに 304 を返す。`/api/container/model` だけは汎用の `/api/{class}/{function}` ではなく専用の経路で返す。`If-None-Match` は `,` 区切りの並び・`*`・弱い比較の `W/"…"` も受け付ける。`file_name` が ASCII の表示文字でない(日本語など)ときは `Content-Disposition: inline; filename="model"; filename*=UTF-8''<百分率符号化>`(RFC 6266)にする。

- 本文はファイルの中身そのまま。`Content-Type` は `media_type`(§3.1)、`Content-Length` は大きさ。
- `ETag` は強い ETag で SHA-256 の 16 進(小文字 64 文字)を引用符で囲んだもの(`"3a7bd3…"`)。
- 要求の `If-None-Match` に同じ ETag(か `*`)があれば `304 Not Modified`(本文なし、`ETag` は付ける)。
- `Cache-Control: no-cache`(使う前に必ず問い合わせる。ファイルが差し替わっても古い形を出さない)。
- `Content-Disposition: inline; filename="<file_name>"`。
- 応答の本文の上限(`option.http_broker.max_body_bytes`)は要求の上限なので、この応答には効かない。上限は §5.4。
- 別の System の Container を HTTP で取り出すときも、中継した System が生のバイト列で返す(BrokerProxy 越しに JSON で受け取り、base64 をほどいて返す)。

### 6.3 JSON の経路(HTTP 以外のブローカーと BrokerProxy)

qmp・IPC・メッセンジャー系のブローカーと、Rust の BrokerProxy・pyjuiz の qmp / IPC のクライアントは、`container/model` を次の JSON で返す。

```json
{"info": { …モデル情報(§8)… }, "data": "<base64>"}
```

- `data` は RFC 4648 §4 の base64(`+` `/`、`=` の詰め物あり、改行なし)。IDL の `bytes` の辞書での形(datatype.md)と同じ。
- 16 MiB 超で WARN(§5.4)。
- `If-None-Match` に当たる仕組みは無い。使い回したい側は先に `container/model_info` の `sha256` を見て、持っているものと同じなら `container/model` を呼ばない。

### 6.4 Rust・pyjuiz・CLI(Step 1・2 で決める)

- Rust: `ContainerBrokerProxy` に `container_model_info(identifier: &str) -> JuizResult<Value>`(モデルが無ければ `Value::Null`)と `container_model(identifier: &str) -> JuizResult<Value>`(§6.3 の `{info, data}`)を足した(Step 1 #381)。バイト列にするのは `juiz_core::models::ContainerModelData::from_value`(`info` と `data: Vec<u8>`、逆は `to_value`)。`identifier` は文字列で、ID でも名前でもよい(Transformations の API と同じ)。
- HTTP の BrokerProxy は `container/model` の生のバイト列(§6.2)と `container/model_info` を続けて取り、`ETag` と `info.sha256` が同じなら `{info, data}` にして返す。違えば(間にファイルが書き換わった)一度だけやり直す。別 System を中継するとき(§6.1)も、`identifier` のブローカー付きの ID を書き換えずに送る(`container/pose` と同じ)。
- 自分の System の中では、HTTP ブローカーは base64 を通さずに中身を取り出して返す(`CRUDDispatcher::container_model_data`)。
- pyjuiz のクライアント: `ContainerProxy.model_info()` → dict(無ければ `None`)、`ContainerProxy.model()` → bytes(Step 2 #382)。
- CLI は §9。

### 6.5 ベンチ(Step 1 #381)

50 MB の .glb を同じ機械の上で取り出した時間(release、5 回の中央値、4 コアのクラウドの Linux、2026-11-14)。測り方は `brokers/qmp_broker/tests/qmp_container_model_test.rs` の `container_model_bench`(`#[ignore]`)。

| 経路 | 時間 |
|---|---|
| HTTP の生のバイト列(§6.2) | 75 ms |
| HTTP の 304(`If-None-Match` が同じ) | 0.6 ms |
| qmp の JSON の経路(base64、§6.3) | 607 ms |
| qmp の `container/model_info` | 0.7 ms |

- 304 は中身を読まずに返す(`If-None-Match` があれば先にモデル情報の SHA-256 だけ比べる。ファイルが変わっていなければ SHA-256 も計算し直さない)。
- JSON の経路は base64 で 4/3 倍になり、JSON の文字列としての組み立てと読み取りが加わるので HTTP の約 8 倍かかる。大きなモデルを何度も取るなら HTTP か、`model_info` の `sha256` を見て取り直さない使い方(§6.3)にする。16 MiB 超の WARN(§5.4)はこのため。

## 7. profile と list

- `container/profile_full` の `model`(§6.1)。Composite の profile・Anchor の profile も同じ。
- `ContainerManifest` に `model: Option<ModelManifest>`(conf の §5.1 の値。`None` なら直列化しない)を足す。manifest に載るのは conf の値で、モデル情報(大きさ・SHA-256)は profile の `model` に載る(Step 1 #381)。
- Step 1 で決めたこと: `ContainerProfile` に `model: Option<Value>` を足した。§6.1 のとおり、モデルが無くても `"model": null` を出す(古い profile は無くても読める)。`ModelManifest` と形式の表(`ModelFormat`: 名前・拡張子・`media_type`・既定の `up_axis`)は juiz_sdk に置き、Step 2 の CLI からも使う。
- `container/profile_full` はモデルのファイルが消えていればエラーになる(§5.3 の「その要求をエラーにする」)。
- `juiz list --long` の Container の行に形式と大きさを出す(Step 2 #382)。

## 8. モデル情報の型 `juiz_std.model.ContainerModelInfo`

`juiz_idl/std/juiz_std/model.jidl` に足す。他の juiz_std の型と同じく、すべてのフィールドに既定値を付け、一度出したら変えない(datatype.md §24.4)。

```text
namespace juiz_std.model;
import idl "juiz_std/geometry.jidl";

/** Container の 3D モデルの付帯情報 (docs/spec/container-model.md) */
record ContainerModelInfo {
  /** 形式: glb / gltf / stl / sdf / other */
  string format = "other";
  /** Content-Type */
  string media_type = "application/octet-stream";
  /** ファイルの大きさ [byte] */
  long size = 0;
  /** ファイルの SHA-256 (16 進、小文字 64 文字) */
  string sha256 = "";
  /** ファイル名 (ディレクトリを除いたもの) */
  string file_name = "";
  /** 1 単位を何 m にするか */
  double scale = 1.0;
  /** Container の座標から見たモデルの取り付け位置 */
  juiz_std.geometry.Transform origin = { … 単位 … };
  /** ファイルの上の軸: y / z */
  string up_axis = "z";
  /** 色を持たない形式の表示色 ("#RRGGBB" / "#RRGGBBAA")。無ければ空 */
  string color = "";
}
```

- `format`・`up_axis` は実際に使う値(既定から決めたものも含む)を入れる。conf で省いたかどうかは区別しない。
- `file_name` だけを出し、ディレクトリは出さない(System を動かす機械のパスを外に見せない)。
- 型の登録所(`type/list`)と OpenAPI の `components.schemas` にいつも載せ、`container/model_info` と `container/profile_full` の `model` の応答はこの型を参照する(Transformations の型と同じ扱い、transformations.md §13.1)。
- Step 1 で決めたこと: `.jidl` は `juiz_idl/std/juiz_std/model.jidl`。`import idl "geometry.jidl";` で `Transform` を読み、`origin` の既定値は `{"translation": {"x": 0.0, "y": 0.0, "z": 0.0}, "rotation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0}}`(geometry.jidl の `TransformStamped` と同じ書式)。指紋は `juiz_std.model.ContainerModelInfo@sha256:500282a80f0f81bd0641ae610d73a89d67cdd51ef94801fb2803e4a4a9b8cda1`(`juiz_idl/tests/idl_test.rs` の `std_type_fingerprints_are_fixed` で固定)。Rust は `juiz_std_types::model::ContainerModelInfo`、Python は pyjuiz 同梱の `juiz_std.model`。
- 型の登録所(`type/list`)では `used_by` が `container_model`。OpenAPI では `container/model_info` の応答と `container/profile_full` の `model` がこの型(か `null`)を参照し、`container/model` の応答は §3.1 の 5 つの Content-Type のバイナリ。

## 9. CLI(Step 2 #382)

### 9.1 `juiz container model`

```
juiz container model <id> [--info] [-o <出力先>]
```

- `-o` が無いか `--info` のときはモデル情報を表示する。
- `-o` があれば中身を書き出す。`-o` がディレクトリなら `<file_name>` で書く。
- HTTP でつないでいるときは HTTP の生の経路(§6.2)、それ以外は JSON の経路(§6.3)を使う。

### 9.2 `juiz model check`

```
juiz model check <file> [--format <形式>] [--up-axis y|z] [--scale <数>]
```

System を起動せずに、モデルのファイルを §3 の規則で確かめる。core の作成時の検査(§5.2)に加えて、次を警告する。

- SDF: 数の個数の誤り、知らない geometry、`<joint>`(§3.2)、`<mesh>`(§3.2.1)、扱えない `relative_to`、入れ子の `<model>`、`<material>` の色が 0〜1 の外。
- glTF: 外部ファイルの参照(§3.3)。
- 外接箱が大きすぎる・小さすぎる(STL・glTF は scale を掛けて 100 m 超・1 mm 未満、SDF は基本形の大きさから)。mm の STL に `scale` を書き忘れたときの注意。

## 10. .juizapp(Step 3 #383)

- モデルのファイルはパッケージのディレクトリに置けば pack に入る(app-package.md §6.2)。形式の変更は無い。
- pack の検査(app-package.md §6.3)に次を足す: entry の conf の各 Container の `model.path` は相対パスで、パッケージの中を指していなければならず(`..` で外に出てはならない、絶対パスは不可)、そのファイルが pack に入るファイル(除外規則の後)になければならない。
- `juiz run` は conf を展開した場所から読むので、conf 相対のパスはそのまま解決される。

## 11. エラー

| エラー | いつ |
|---|---|
| `ContainerModelInvalidError` | `model` のキーの型・値の誤り(§5.1)、上限超え(§5.4)、形式の判定に合わない(§3.1)、API からの作成に `model`(§5.5) |
| `ContainerModelFileNotFoundError` | `path` のファイルが無い・開けない(作成時、取り出し時) |
| `ContainerHasNoModelError` | モデルの無い Container に `container/model` |

HTTP では `ContainerModelFileNotFoundError` と `ContainerHasNoModelError` を 404、`ContainerModelInvalidError` を 400(作成時)・500(取り出し時)にする。

Step 1 で決めたこと: それまでの HTTP ブローカーはエラーをすべて 500 で返していた(対応表は無かった)。エラーの応答を作る所(`http_router::error_status`)で、エラーの連なりの中の `JuizError` を見て上の 3 つだけを振り分け、それ以外は今までどおり 500 にした。「作成時」は POST(`*/create`)の要求のこと。404 はどの経路(`container/model`・`model_info`・`profile_full`)でも同じ。本文は今までどおり `{"message": …}`。

ローカルブローカー・qmp などのメッセンジャー系の BrokerProxy では、相手のエラーは文(`BrokerProxyRemoteError`)で届く(既存の扱い)。

## 12. 管理画面の 3D 表示(Step 4 #384)

- 新しい表示「3D」。Anchor を選ぶと、その木の Container を §4.4 の位置に置き、モデルがあればモデル、無ければ(または表示できない形式・`<mesh>` だけの SDF なら)小さな座標軸(x 赤・y 緑・z 青)と名前を出す。Transform は親子を結ぶ線。
- 描画は three.js(GLTFLoader・STLLoader、MIT)を管理画面に同梱する(CDN は使わない)。3D 表示を開いたときだけ読み込む(動的 import)。SDF は §3.2 の部分集合を XML から読み、three.js の基本形にする。
- モデルは HTTP の `container/model` を ETag 付きで取り、SHA-256 ごとに 1 回だけ読み込んで使い回す(同じ形の Container が 10 台あっても 1 回)。
- 位置は `transformations/tree?anchor=` の問い合わせ(5〜10 Hz)で更新する。押し出し(SSE / WebSocket)は将来。
- 3D 表示で Container を選ぶと、既存の右側の詳細(profile・ContainerProcess・Pose)を出す。

## 13. 範囲の外(将来)

- core でのメッシュの読み込み・変換・当たり判定。
- 複数ファイルのモデル(外部参照の .gltf、SDF の `<mesh>`)の配信。
- API からのモデルの指定・実行中の差し替え・動く形(点群など)。Step 10 #389。
- 型(Factory)の既定のモデル(プラグインの manifest に `model` を書く)。Step 8 #387。
- URDF・複数 link の SDF の取り込み(`juiz import urdf|sdf`、link → Container、joint → Transform、`package://`・`model://` の解決)。Step 9 #388。
- 位置の押し出し配信。
- 1 Container に複数のモデル(見た目用・当たり判定用など、§2 の 2)。

## 14. 他の機能との関係

- **Transformations**: モデルの置き場所は ContainerPose。3D 表示の位置の出どころは `transformations/tree`。座標の約束は transformations.md §3.1 と同じ。
- **Composition**: CompositeContainer も Container なのでモデルを持てる(全体の外形)。スロットの Container はそれぞれ自分のモデル。
- **ネームスペース**: 複製したロボットごとに同じモデルのファイルを指してよい。管理画面は SHA-256 で 1 回だけ取る。
- **データ型**: モデル情報は `juiz_std.model.ContainerModelInfo`。中身には型を付けない(バイト列)。
- **.juizapp**: §10。

## 15. Step と担当する節

| Step | issue | 節 |
|---|---|---|
| 0 仕様書 | #380 | この文書 |
| 1 SDK・core・API | #381 | §3.1・§3.3 の作成時の警告・§5・§6.1〜§6.4 の Rust・§7・§8・§11 |
| 2 CLI・pyjuiz | #382 | §6.4 の pyjuiz・§7 の list・§9 |
| 3 .juizapp の検査 | #383 | §10 |
| 4 管理画面の 3D 表示 | #384 | §3.2・§4・§12 |
| 5 サンプル | #385 | Transformations のサンプル(map → odom → 台車 → 雲台 → カメラ)にモデルを足す。モデルは単純な形をスクリプトで作った .glb / .stl と手書きの .sdf |
| 6 公式サイト | site#65 | ガイド container-model |
| 7 リリース | #386 | |
