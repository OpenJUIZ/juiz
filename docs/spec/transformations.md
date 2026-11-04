# Transformations(Container の位置姿勢と座標変換)仕様

- 状態: 確定(Step 0、issue #367、2026-11-02)
- 親 issue: #366。計画と経緯はプロジェクトの計画書 transformations-plan.md(2026-11-02 確定)にある
- この文書は Step 1〜7(#368〜#374)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- §19(System をまたぐ Transform)は issue #413(#376 の Step 0、2026-11-03)で足した。計画はプロジェクトの計画書 transformations-cross-system-plan.md(2026-11-03 確定)。§19 は Step #414〜#418 の実装の基準になる
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す。「(Step n で決める)」と書いた所は、その Step の PR でこの文書に書き足す

## 1. 目的と用語

ロボットの部品(台車、腕の関節、カメラ、センサ)を Container で表し、その位置姿勢と部品同士の位置関係を juiz の中で扱えるようにする。この機能全体を **Transformations** と呼ぶ。

| 用語 | 意味 |
|---|---|
| **ContainerPose** | Container の位置そのもの。位置 (x, y, z) と四元数 (x, y, z, w) の姿勢。**必ず基準 Container を伴う**(「どの Container から見た位置か」)。すべての Container が持つ |
| **Transform** | Container 同士を結ぶ**有向**の接続。3 次元のオフセットと四元数の回転を持ち、始点(親)から見た終点(子)の位置姿勢を表す |
| **Anchor** | 座標変換の原点になる特別な Container。利用者の状態はほぼ空。名前を持ち、1 つのシステムに複数あってよい。Transformations Tree の根になる |
| **PeriodicExecutionAnchor** | 自分の周期 `rate` で木をたどり、木の Container の ContainerPose を更新する Anchor。type_name は `periodic_anchor` |
| **StaticAnchor** | 作られたとき・木の形が変わったとき・明示的に頼まれたときだけ木の ContainerPose を更新する Anchor。type_name は `static_anchor` |
| **Transformations Tree** | ある Anchor を根として Transform を始点から終点へたどって届く Container の木。「A の木」と書く |
| **Transformations 登録所** | System が持つ、Transform の一覧・木の形・Pose 通知の登録をまとめたもの(§9) |
| **Pose 通知** | Container の ContainerPose が明示的に書かれたときに、登録した Process を呼ぶこと(§8) |

ROS の tf との違い: tf の frame は Container とは別の名前だが、juiz では **Container そのものが座標系**である。Container の名前がそのまま座標系の名前になり、別に frame の名前は持たない。

## 2. 基本方針

1. 位置姿勢は Container の「位置そのもの」で、名前は ContainerPose。必ず基準 Container の名前と組で読み書きする(読むときも書くときも基準を指定する)。
2. Anchor は Container の一種である。ID・ネームスペース・list / profile・管理画面の表示・削除は Container と同じ仕組みに乗る。
3. Transform の木と更新は 1 つの System の中で閉じる。System をまたぐ Transform は終点(子)の System が持ち、親の位置を「写し」として取り込んで自分の System の中の木として扱う(§19)。木の形が System の外へ広がることはない。
4. ContainerPose は利用者の状態とは**別のロック**に置く(§9.2)。ContainerProcess が長く走っていても Anchor の更新は止まらない。
5. 値の形は `juiz_std.geometry` の型(`Pose`・`PoseStamped`・`Transform`・`TransformStamped`)に合わせる。通信で流れる辞書もこの形。
6. 他言語のプラグインは**今の関数の形を変えない**。呼び出しに渡している引数に呼び出し元 Container の口を隠して載せる(§12)。

## 3. 値と計算規則

### 3.1 値の形

位置姿勢 P = (t, q) は、並進 t = (x, y, z) [m] と単位四元数 q = (x, y, z, w) の組とする。

- 四元数の並びは **(x, y, z, w)**(w が最後)。JSON・辞書・C の構造体・引数の順番のすべてで同じ。ROS・`juiz_std.geometry.Quaternion` と同じ。
- 回転なしは (0, 0, 0, 1)。
- 保存するとき(Transform の作成・更新、`set_pose`、Anchor の更新)に**正規化しなければならない**。長さが 0(1e-12 未満)の四元数、NaN・無限大を含む値はエラー(`InvalidPoseValueError`)。
- q と −q は同じ回転を表す。比べるときは回転として比べる(テストで値をそのまま比べない)。正規化のときに w ≥ 0 へそろえることはしない。

| 値 | 辞書の形(型) |
|---|---|
| ContainerPose | `juiz_std.geometry.Pose` `{"position": {x,y,z}, "orientation": {x,y,z,w}}` |
| 時刻と基準つきの ContainerPose | `juiz_std.geometry.PoseStamped`。`header.frame_id` は基準 Container の名前(§5.4)、`header.stamp` は計算・書き込みの時刻 |
| Transform の値 | `juiz_std.geometry.Transform` `{"translation": {x,y,z}, "rotation": {x,y,z,w}}` |
| 時刻と端点つきの Transform | `juiz_std.geometry.TransformStamped`。`header.frame_id` が始点、`child_frame_id` が終点 |

`Transform` / `TransformStamped` は ROS 2 の `geometry_msgs/Transform` / `TransformStamped` と同じ名前・同じ辞書の形で、`juiz-idl` の ROS 2 取り込みでも juiz_std に置き換わる(datatype.md §25.2)。

### 3.2 向き

Transform(P→C) = (t, q) は「子 C の座標を親 P の座標へ写す」変換とする(ROS と同じ):

```text
p_P = q · p_C · q⁻¹ + t        (以下 q · p と略す。p_C は C から見た点、p_P は同じ点を P から見たもの)
```

言い換えると、t は P から見た C の原点の位置、q は P から見た C の向きである。Transform(P→C) の値は「P を基準にした C の ContainerPose」と同じ数になる。

### 3.3 合成と逆

```text
(t1, q1) ∘ (t2, q2) = (t1 + q1 · t2,  q1 ⊗ q2)        (⊗ はハミルトン積)
(t, q)⁻¹            = (−(q⁻¹ · t),   q⁻¹)             (単位四元数の q⁻¹ は共役 (−x, −y, −z, w))
単位               = ((0, 0, 0), (0, 0, 0, 1))
```

### 3.4 計算規則

```text
pose_A(A)     = 単位                                  (Anchor は自分から見て原点)
pose_A(C)     = pose_A(P) ∘ Transform(P→C)            (P は A の木での C の親)
pose_R(C)     = pose_A(R)⁻¹ ∘ pose_A(C)               (R と C が同じ Anchor A の木にあるとき)
pose_C(C)     = 単位
```

`pose_R(C)` は「R を基準にした C の ContainerPose」と読む。

### 3.5 数値の例(全言語のテストで同じ値を確かめる)

木: `map`(Anchor)→ `base0` → `camera0`

| Transform | translation | rotation (x, y, z, w) |
|---|---|---|
| map → base0 | (1, 0, 0) | z 軸まわり 90°: (0, 0, 0.7071067811865476, 0.7071067811865476) |
| base0 → camera0 | (0.1, 0, 0.3) | (0, 0, 0, 1) |

- `pose_map(camera0)` = position (1, 0.1, 0.3)、orientation (0, 0, 0.7071067811865476, 0.7071067811865476)
- `pose_base0(camera0)` = position (0.1, 0, 0.3)、orientation (0, 0, 0, 1)
- `pose_camera0(map)` = position (−0.1, 1, −0.3)、orientation (0, 0, −0.7071067811865476, 0.7071067811865476)
- `camera0` に `set_pose("map", position (1, 0.2, 0.3)、orientation (0, 0, 0.7071067811865476, 0.7071067811865476))` をすると、Transform(base0→camera0) が translation (0.2, 0, 0.3)、rotation (0, 0, 0, 1) に書き換わる

比べるときの許容誤差は 1e-9。

## 4. Anchor

### 4.1 Anchor = 特別な Container

- Anchor は Container である(`JuizObjectClass::Container`)。ID は普通の Container と同じ `core://core/container/<ns>/<name>::<type_name>`。
- type_name は `periodic_anchor` か `static_anchor`。どちらも System に組み込みで、プラグインは要らない。
- 利用者の状態は持たない。持つのは更新方式と(Periodic なら)周期だけ。
- この版では Anchor に ContainerProcess を付けられない(conf の `processes:` を書くとエラー)。
- 作成・削除は Container と同じ(conf の `containers:`、API の `container/create`・`container/destroy`)。
- `list --long`・`profile_full` では普通の Container と同じ形に、`anchor: {"kind": "periodic" | "static", "rate": <Hz>}` を足す。

### 4.2 Anchor を別の Anchor の子にする

Anchor を別の Anchor の Transform の終点にしてよい(例: `map → odom → base0`、ROS の map / odom と同じ形)。このとき `odom` は自分の木の根であり、`map` の木の途中のノードでもある。`odom` の木は `odom` から届く範囲だけで、`map` は入らない。

### 4.3 PeriodicExecutionAnchor

- manifest: `rate`(Hz、正の数)。省略時は 10.0。0 以下・NaN はエラー。
- System の起動(conf の読み込みが終わった後)で自分のスレッドを始め、`1 / rate` 秒ごとに自分の木を更新する(§6)。中身は `timer_ec` と同じ仕組みでよい。System の停止・Anchor の削除で止める。
- 1 回の更新が周期より長くかかったときは、次の更新をすぐに始める(遅れを貯めて連続で走らせない)。

### 4.4 StaticAnchor

次のときだけ自分の木を更新する:

1. 作られたとき。
2. 自分の木の形または値が変わったとき: 自分の木に届く Transform の作成・削除・値の変更(`set_pose` による書き換えを含む)、木の中の Container の削除。
3. 明示的に頼まれたとき(API `anchor/update`、CLI `juiz anchor update`)。

2 は変更をした呼び出しの中で、戻る前に行う(`set_pose` が戻った時点で StaticAnchor の木は新しい値になっている)。

## 5. 木の規則

### 5.1 Transform

- Transform は始点 `source` と終点 `destination` の Container、値(translation・rotation)を持つ。
- ID は `transform://<source の ID>|<destination の ID>`(接続 `connection://<src>|<arg>|<dst>` にならう)。
- 同じ (source, destination) の Transform を 2 つ作るのはエラー(`TransformAlreadyExistsError`)。値を変えるときは update。
- source と destination が同じ Container はエラー。
- 端点はどちらも同じ System の Container(Anchor を含む)でなければならない。見つからなければエラー。ただし始点は別 System の Container でもよい(System をまたぐ Transform、§19)。終点はいつも自分の System の Container。
- ContainerProcess・Process は端点にならない。

### 5.2 Anchor ごとに届く範囲が木であること

Anchor A から Transform を始点 → 終点へたどって届く Container の集合を reach(A) とする。**すべての Anchor A について、reach(A) と、その中の 2 点を結ぶ Transform が、A を根とする木になっていなければならない**:

- reach(A) の A 以外の Container は、reach(A) の中に始点を持つ Transform を**ちょうど 1 本**持つ(2 通りの道で届くのはエラー)。
- A に、reach(A) の中から入る Transform があってはならない(循環はエラー)。

許すもの:

- 1 つの Container が複数の Anchor の木に入ること(`map → base0` と `world → base0` は、`map` と `world` が互いに届かなければよい)。
- reach(A) の外から入る Transform(例の `world → base0` は `map` の木から見ると外)。
- どの Anchor からも届かない Transform(使われないが、エラーにはしない。後で Anchor から届くようになったときに検査する)。

検査は Transform の作成・source / destination の変更のたびに、影響を受ける Anchor 全部について行う。破る変更は**丸ごと**拒む(`TransformTreeError`、どの Anchor のどの Container で破れたかを入れる)。conf の読み込みでは `transforms:` をすべて登録した後に検査する。

### 5.3 木での親

A の木での C の親 parent_A(C) は、reach(A) の中に始点を持ち C を終点とするただ 1 本の Transform の始点である。

### 5.4 名前の解決と header.frame_id

- 基準 Container・Transform の端点・Anchor は、ID か名前で指定する。名前の解決は conf・API の他の所と同じ(ID ネームスペースの規則、#146: ネームスペースつきの名前 `<ns>/<name>`、省略時は呼び出し元と同じネームスペース、type_name は省略可。同じ名前が 2 つあればエラー)。
- `PoseStamped.header.frame_id`・`TransformStamped` の `frame_id` / `child_frame_id` には Container の名前を入れる。ネームスペースがあるときは `<ns>/<name>`。

## 6. Anchor の更新

### 6.1 各 Container が持つ値

各 Container は「Anchor の ID → 値」の表(PoseState)を持つ。値は (pose_A(C)、時刻 stamp、版番号)。Anchor 自身も自分の項目(単位)を持つ。

### 6.2 更新の手順

Anchor A の更新は、Transformations 登録所のロックの下で次を行う:

1. A から幅優先で木をたどり、各 C について `pose_A(C) = pose_A(parent_A(C)) ∘ Transform(parent_A(C)→C)` を計算して C の PoseState[A] に書く。stamp は更新を始めた時刻(木全体で同じ値)。
2. 前回の更新から、親の値も Transform の値も変わっていない部分木は飛ばしてよい(版番号で判断する)。
3. 前回は reach(A) にあって今回はない Container からは、PoseState[A] を消す。

更新は Container の利用者の状態のロックを取らない(§9.2)。

## 7. 読み書き

### 7.1 読む `pose(C, R)`

Container C の、基準 R から見た ContainerPose を返す。

1. C = R なら単位。
2. R が Anchor で C ∈ reach(R) なら、C の PoseState[R] をそのまま返す(登録所に問い合わせない、速い)。まだ値がなければ(作られた直後の Periodic など)その場で計算する。
3. それ以外は、R と C の両方を含む Anchor A を探し、`pose_A(R)⁻¹ ∘ pose_A(C)` を返す(値は PoseState[A] を使う)。候補が複数あれば、A の木での R と C の深さの和が最小のもの、同じなら Anchor の ID の文字列順で最初のものを使う。
4. 見つからなければエラー(`NoCommonAnchorError`)。

戻り値は `PoseStamped`。stamp は使った値の時刻(3 では 2 つの古い方)。

Periodic の値は最大 1 周期古い。読んだ時点の最新が必要なら、Static を使うか `anchor/update` を頼む。

### 7.2 書く `set_pose(C, R, p)`

Container C を、基準 R から見て p の位置姿勢にする。

1. 7.1 の 2・3 と同じ規則で、R と C を含む Anchor A を選ぶ(R が Anchor で C ∈ reach(R) なら A = R)。見つからなければエラー(`NoCommonAnchorError`)。どの木にも入っていない C への書き込みはここでエラーになる。**Transform を自動では作らない。**
2. C = A(C が A の木の根)、C = R はエラー(`PoseNotWritableError`)。
3. R が A の木で C の子孫ならエラー(`PoseNotWritableError`。C を動かすと R も動くため)。
4. P = parent_A(C) として、Transform(P→C) の値を `pose_A(P)⁻¹ ∘ pose_A(R) ∘ p` に書き換える(正規化する)。このとき pose_A(P)・pose_A(R) は PoseState の値ではなく、登録所のロックの下でその場で計算した値を使う。
5. C の PoseState[A] をすぐ `pose_A(R) ∘ p` にする。
6. Transform(P→C) を木に持つ他の Anchor について: StaticAnchor はすぐ更新する(§4.4 の 2)。PeriodicExecutionAnchor は次の周期で追従する(C の子孫も同じ)。
7. Pose 通知を出す(§8)。

1〜6 は登録所のロックを 1 回取ったまま行い、周期更新と混ざらない。戻り値は書いた後の `pose(C, R)`(`PoseStamped`)。

関節の角度のように、`set_pose` は C の値そのものではなく**親 → C の Transform を動かす**ことに注意。Transform を API で直接変えても同じ結果になる。

### 7.3 Transform の値の変更

API `transform/update` で値を変えると、§7.2 の 5・6・7 と同じことが起きる(C は終点、通知の基準は始点)。

## 8. Pose 通知

### 8.1 きっかけ

**明示的な書き込み**だけが通知のきっかけになる:

- `set_pose`(ContainerProcess から、API・CLI から)
- Transform の作成・値の変更・削除(終点の Container について通知する)

Anchor の周期更新・Static の再計算で値が変わっても通知しない。ただし登録で `on_update: true` にした Pose 通知は、Anchor の更新で PoseState の値が変わったときにも呼ぶ(§8.3)。

### 8.2 受け手

1. **Anchor(内部)**: StaticAnchor が自分の木を更新し直す(§4.4)。利用者には見えない。
2. **登録した Process**(Pose 通知、pose listener): 「Container X の Pose が書かれたら Process Y を呼ぶ」という登録。

### 8.3 Pose 通知の登録

| 項目 | 内容 |
|---|---|
| `container` | 見張る Container X(必須) |
| `process` | 呼ぶ Process Y(必須)。ContainerProcess も可。ブローカー越しの別プロセスの Process も可(push 接続と同じ仕組み) |
| `arg` | Y の引数の名前。Y の引数が 1 つなら省略可、2 つ以上なら必須 |
| `reference` | 渡す値の基準 Container。省略時は書き込みで指定された基準(Transform の変更なら始点) |
| `on_update` | `true` なら Anchor の更新で値が変わったときも呼ぶ。既定 `false`。このとき `reference` は必須 |

- ID は `pose_listener://<X の ID>|<Y の ID>|<arg>`。同じ組の二重登録はエラー。
- Y の引数には `pose(X, reference)` の値を `juiz_std.geometry.PoseStamped` で渡す。他の引数は Y の既定値か今の値(push 接続で `execute` するのと同じ)。
- 呼ぶのは書き込みを確定させた後、登録所のロックを外してから、書き込んだスレッドで行う。Y のエラーは WARN を出して捨て、書き込みは成功のまま。
- Y の引数の型が `juiz_std.geometry.PoseStamped` 以外(か any 以外)なら、登録のときにエラー(データ型の接続時の型検査と同じ規則)。

### 8.4 再入の止め方

Pose 通知で呼ばれた Process の中でまた `set_pose` すると、その書き込みは行うが、**その書き込みによる Pose 通知は出さず**、WARN を 1 回出す(通知の中での再入は 1 段で止める)。StaticAnchor の更新(内部の受け手)は止めない。判定はスレッドごとの深さで行う(ブローカー越しの別プロセスの Process からの書き込みは、別の書き込みとして扱う)。

## 9. 置き場所とロック

### 9.1 置き場所

- PoseState(§6.1)は `Arc<RwLock<PoseState>>` に入れ、`ContainerImpl` と `ContainerPtr` の両方から同じものを指す。ホスト側の他言語の Container(`ContainerImpl<PythonContainerStruct>` など)にも同じく付くので、全言語の Container が ContainerPose を持つ。
- Transform の一覧・木の形・各 Anchor の reach・Pose 通知の登録は System の **Transformations 登録所**が持つ。
- Container は System に登録されたときに登録所への口を受け取る。登録されていない Container の `pose` / `set_pose` はエラー(`TransformationsNotAttachedError`)。

Step 1(#368)で決めた SDK の形(`juiz_sdk::transformations`):

- `PoseState`: Anchor の ID → `PoseEntry { pose, stamp, version }`。版番号は PoseState ごとに書くたびに増える。`PoseStatePtr = Arc<RwLock<PoseState>>`。
- `TransformationsRegistry`(trait、`Send + Sync`): `pose(&ContainerIdentifier, reference)` と `set_pose(&ContainerIdentifier, reference, &Pose)`。core の登録所(Step 2)が実装する。計算規則(§7)はすべて実装側の責任。
- `ContainerPoseHandle`: Container ごとの PoseState と登録所への口の組。`ContainerImpl::new` が作り、`Container::pose_handle()`(trait の必須メソッド)で取り出せる。`ContainerPtr::new` はこれを受け取って持つので、`ContainerPtr::pose` / `set_pose` / `pose_state` / `attach_transformations` / `detach_transformations` は Container のロックを取らない。登録所は循環しないよう `Weak` で持つ(登録所が消えれば `TransformationsNotAttachedError`)。`detach` は PoseState も空にする。
- `ContainerPtr` は `Send` でない(`dyn Container` が `Send` でない)ので、Periodic Anchor のスレッドから使う登録所は `ContainerPtr` ではなく `ContainerPoseHandle`(`Send + Sync`)を持つ。
- 読み出しだけの口 `ContainerPoseReader`(`pose`・`entry` だけを持つ)を `pose_reader()` で取り出せる(§14)。
- 別 System の Container(`ContainerProxy`)も口を持つが、登録所にはつながない。別 System の Container の読み書きはその System に回す(§19.8)。

### 9.2 ロックの順番

- ContainerProcess は呼び出しの間ずっと Container の書き込みロックを持つ。PoseState をそのロックの中に置くと、時間のかかる ContainerProcess が終わるまで Anchor の更新が止まるので、別のロックにする。
- ロックを取る順番は **Container の利用者の状態 → 登録所 → PoseState** に限る。登録所と PoseState は、Container の利用者の状態のロックを**取ってはならない**(ContainerProcess の中からの `set_pose` が登録所のロックを取るため、逆順に取るとデッドロックする)。
- PoseState のロックは値の読み書きの間だけ持つ。
- Anchor の更新と `set_pose` の Transform 書き換えは、どちらも登録所のロックの下で行う(混ざらない)。

### 9.3 core の形(Step 2、#369 で決めたこと)

- 登録所は `juiz_core::transformations::Transformations`(`CoreStore::transformations`、`Arc` で持つ)。中身は 1 つの `Mutex` で、Transform の一覧・各 Anchor の木(幅優先の順・親・深さ)・Pose 通知の登録を持つ。Container は `ContainerPoseHandle` だけで持ち、`ContainerPtr` は持たない(§9.1)。
- Container は `CoreWorker::create_container_ref`・`create_composite_container` で System に登録したときに登録所につなぎ、`destroy_container_ref` で外す。
- Anchor は Factory を持たず、`create_container_ref` が type_name を見て直接作る(`AnchorContainer = ContainerImpl<AnchorState>`)。`ContainerProfile` に `anchor`(Anchor 以外では出力しない)を足した。
- Periodic Anchor のスレッドは `System::setup` の最後(EC の起動の後)に始める。起動後に作った Periodic はすぐ始める。スレッドは登録所を `Weak` で持ち、止めるときは通知を送ってから登録所のロックの外で join する。
- 版番号による省略(§6.2 の 2): Anchor ごとに Container ごとの「親の PoseState[A] の版番号と、親 → 子の Transform の版番号」を覚え、どちらも同じなら計算も書き込みもしない。飛ばした Container の stamp は前の値のまま。Transform の版番号は登録所の中で一意。
- 木から外れた Container の PoseState の項目は、次の更新を待たず、木の形が変わったときにすぐ消す(§6.2 の 3)。
- 名前の解決(§5.4)は `<name>`・`<ns>/<name>`・`/<絶対の ns>/<name>` に加えて `<name>::<type_name>` も受け付ける。conf の `transforms:`・`pose_listeners:` は System の既定ネームスペースから解決する。
- `set_pose` の戻り値は、PoseState ではなく登録所のロックの下でその場で計算した pose_A(R) から求めた値(Periodic の木でも書いた値がそのまま返る)。
- Container を削除して消えた Transform についても、残った終点に Pose 通知を出す(§8.1 の削除と同じ)。値を計算できない(共通 Anchor がなくなった)通知は出さない(DEBUG ログ)。
- Pose 通知の受け手が、書き込みをしている ContainerProcess と同じ Container の ContainerProcess だと、その Container の書き込みロックを待って止まる。この版では検出しないので、そういう登録はしないこと。
- core の関数(API・CLI は Step 3): `Transformations::create_transforms`(conf と `transform/create`、まとめて作り 1 つでも不備があれば何も作らない)・`update_transform`・`destroy_transform`・`transform_list`・`transform_profile`・`update_anchor`・`anchor_list`・`tree`・`pose_by_name`・`set_pose_by_name`・`pose_listener_list`・`pose_listener_profile`・`destroy_pose_listener`、`CoreWorker::create_pose_listener`。
- 1000 ノードの木(各ノードが子を 10 個持つ、StaticAnchor)の 1 回の更新(release、`tests/integration/transformations_bench.rs`): 根の直下の Transform を変えたとき(全ノードを計算し直す)約 0.95 ms、何も変わっていないとき(全部飛ばす)約 0.48 ms。1000 本の Transform をまとめて作る(検査と最初の更新を含む)のは約 235 ms。

## 10. 後始末

- Container を削除すると、それを端点に持つ Transform と、それを見張る・それを呼ぶ Pose 通知を消す。影響を受ける StaticAnchor は更新し、Periodic は次の周期で追従する。
- Anchor を削除すると、自分のスレッドを止め、すべての Container の PoseState から自分の項目を消す。
- Transform を削除すると、木から外れた Container の PoseState からその Anchor の項目を消す(§6.2 の 3)。

## 11. conf

```yaml
containers:
  - type_name: periodic_anchor
    name: map
    rate: 30.0
  - type_name: static_anchor
    name: odom
  - type_name: mobile_base
    name: base0
  - type_name: pantilt
    name: pantilt0
  - type_name: camera
    name: camera0
transforms:
  - source: map
    destination: odom
  - source: odom
    destination: base0
    translation: {x: 0, y: 0, z: 0}
    rotation: {x: 0, y: 0, z: 0, w: 1}
  - source: base0
    destination: pantilt0
    translation: {x: 0.1, y: 0, z: 0.3}
  - source: pantilt0
    destination: camera0
    translation: {x: 0.05, y: 0, z: 0.02}
pose_listeners:
  - container: base0
    process: map_view0
    arg: pose
    reference: map
```

- `transforms:` の `translation` / `rotation` は省略時それぞれ 0 と回転なし。辞書のキーは `juiz_std.geometry.Transform` と同じ。
- SDK の manifest(Step 1、`juiz_sdk::manifests`):
  - Anchor の `rate` は `ContainerManifest.rate: Option<f64>`(Anchor 以外では使わず、`None` なら直列化しない)。`ContainerManifest::anchor_manifest()` が `AnchorManifest { kind: AnchorKind, rate: Option<f64> }` を返す(Anchor でなければ `None`)。§4.3 の既定値と検査、§4.1 の `processes:` の禁止はここで行い、不備は `ContainerManifestInvalidError`。Static に `rate` を書くと WARN を出して使わない。`AnchorManifest::to_profile_value()` が §4.1 の `anchor` の値を返す。定数 `PERIODIC_ANCHOR_TYPE_NAME`・`STATIC_ANCHOR_TYPE_NAME`・`DEFAULT_ANCHOR_RATE`。
  - `TransformManifest { source, destination, translation, rotation }`、`PoseListenerManifest { container, process, arg, reference, on_update }`。`list_from_conf(&conf)` で conf 全体から `transforms:` / `pose_listeners:` を読む(なければ空)。1 項目だけで分かる不備(端点が空・同じ名前、`on_update` で `reference` なし など)はここで検査し、名前の解決・木の規則・引数の型は core(Step 2)が検査する。Transform の回転はここで正規化する。
  - ID は `transform_identifier(src, dst)`・`pose_listener_identifier(x, y, arg)` で作る。
- `transforms:` は `containers:` の後、`pose_listeners:` は `processes:`・`containers:` の後に読む。
- 名前の解決は §5.4。
- `transforms:` の端点が見つからない、木の規則(§5.2)を破る、`pose_listeners:` の Process が見つからない、はどれも conf の読み込みエラーとする(他のエラーと同じく ERROR を出す。起動を止めるかは他の conf のエラーと同じ扱い)。
- `.juizapp` は conf にこれらが入るだけで、形式は変えない。

## 12. 言語ごとの書き方

### 12.1 Rust

```rust
#[juiz_component_container_process(container_type = "mobile_base")]
fn mobile_base_update(c: &mut ContainerImpl<MobileBase>, odom: Value) -> JuizResult<Capsule> {
    let p = c.pose("map")?;          // PoseStamped 相当
    c.set_pose("map", new_pose)?;     // 書く
    ...
}
```

- 読む口は `&self`、書く口は `&mut self` のメソッドにする(§14)。
- 値の型は `juiz_sdk::geometry`(Step 1、#368 で f64 に作り直した。nalgebra・quaternion-core への依存はやめた):

  | 型 | juiz_std の型 | 主なメソッド |
  |---|---|---|
  | `Vector3`(別名 `Point3D`) | `Vector3` / `Point3D` | `+` `-`、`dot`・`cross`・`norm` |
  | `Quaternion` | `Quaternion` | `identity`・`from_axis_angle`・`from_rpy` / `to_rpy`(ROS の RPY)・`normalized`・`inverse`・`rotate`・`angle_to`、`*` はハミルトン積 |
  | `Transform` | `Transform` | `identity`・`compose`(`*`)・`inverse`・`transform_point`・`normalized`・`approx_eq` |
  | `Pose` | `Pose` | `Transform` と同じ計算。`relative(&other)` は `self⁻¹ ∘ other`(`pose_R(C)` の計算)。`Pose * Transform` は `Pose` |
  | `Time`・`Header`・`PoseStamped`・`TransformStamped` | `juiz_std.common.Time`・`Header`、`PoseStamped`・`TransformStamped` | `Time::now()`、`PoseStamped::reference()`(= `header.frame_id`) |

  どれも `Value`(辞書)・`Capsule` と `From` / `TryFrom` で行き来できる。足りないキーは juiz_std の既定値で埋め、形が違えば `DataConversionError`。`juiz_std_types` の型とはこの辞書で行き来する(`juiz_sdk` は `juiz_std_types` に依存できないため)。geometry の型は prelude に入れない(`juiz_std_types` の同名の型とぶつかるため)。
- `ContainerImpl<S>` のメソッド: `pose(&self, reference: &str) -> JuizResult<PoseStamped>`、`set_pose(&mut self, reference: &str, pose: Pose) -> JuizResult<PoseStamped>`、`pose_reader(&self)`(読み出しだけの口)、`pose_state(&self)`。`set_pose` は値を正規化してから登録所に渡す(不正な値は登録所に届く前に `InvalidPoseValueError`)。

### 12.2 Python

ContainerProcess の第 1 引数(利用者のオブジェクト)からたどれる口で読み書きする。

```python
from juiz import juiz_container_process, pose, set_pose
from juiz_std.geometry import PoseStamped

@juiz_container_process(container_type="mobile_base")
def mobile_base_update(base, dx: float = 0.0) -> PoseStamped:
    p = pose(base, "map")                  # map から見た base の ContainerPose (PoseStamped)
    p.pose.position.x += dx
    return set_pose(base, "map", p.pose)   # 親 → base の Transform を書き換え、書いた後の値を返す
```

Step 4(#371)で決めたこと:

- 関数は pyjuiz の `juiz.pose(container, reference)`(読む)と `juiz.set_pose(container, reference, value)`(書く)。読み出しと書き込みは別の関数(§14)。`juiz/transformations.py`。
- 口: ホストは Python の Container を作ったとき(`load_container_factory` の constructor)、利用者のオブジェクトに属性 `__juiz_pose__` としてホストの `ContainerPoseHandle` の包み(PyO3 のクラス `juiz.ContainerPoseHandle`、`juiz_core/src/plugin/python/pose.rs`)を付ける。`juiz.pose` はこの属性を通して呼ぶので、ContainerProcess の中だけでなく、同じオブジェクトを持っていればどこからでも呼べる。pip 版(juiz-cli)と埋め込み版で同じ仕組み(どちらもホストの juiz_core が付ける)。
- 属性を付けられないオブジェクト(`__slots__` を持つクラス、dict などの組み込みの型)では WARN を出して付けず、`juiz.pose` は `TransformationsNotAttachedError` になる。Container の constructor は普通のクラスのオブジェクトを返すこと。
- `reference` は Container の名前か ID の文字列。`identifier()` を持つもの(ContainerProxy など)も受け付ける。
- 戻り値はどちらも `juiz_std.geometry.PoseStamped`(型の値)。`set_pose` の `value` は `Pose`・`PoseStamped`・それぞれの辞書を受け付ける。`PoseStamped` は `pose` だけを使い、`header` は見ない(基準はいつも `reference`)。
- ホストと pyjuiz の間は辞書で行き来する。ホストは登録所を呼ぶ間 GIL を手放す(Pose 通知の受け手が Python の Process でも、そのスレッドで GIL を取り直して呼ぶ)。
- エラーは §17 の名前の例外: `juiz.NoCommonAnchorError`・`PoseNotWritableError`・`InvalidPoseValueError`・`TransformationsNotAttachedError`、基底は `juiz.TransformationsError`(`RuntimeError` の子、`kind` に種類の名前)。ホストは `RuntimeError(種類, メッセージ)` を投げ、pyjuiz が振り分ける。
- あわせて、Python の Container の Factory だけを読むときにも `pythonpath` を足すようにした(以前は ContainerProcess を先に読んでいないと `import juiz` できなかった)。

### 12.3 C ABI(C++・Haskell・Rust C ABI プラグイン)

- **今の入口の関数の形は変えない**(C++ の `entry_point(cobj, CapsuleMap*, Capsule*)`、Rust C ABI の frame_in / frame_out)。
- ホストは ContainerProcess を呼ぶとき、呼び出しに渡している引数(C++・Haskell は CapsuleMap、Rust C ABI は frame_in)に、呼び出し元 Container の口(PoseState と登録所への不透明なハンドル)を隠して載せる。利用者から見える引数は増えない。
- cjuiz(`juiz_sdk.h`)に、その口を取り出して読み書きするアクセサ関数 `juiz_container_pose_get` / `juiz_container_pose_set` を足す。四元数の順番は (x, y, z, w)。
- 口を載せない古いホスト・口を知らない古いプラグインの組み合わせでも、今まで通り動かなければならない(口がなければアクセサはエラーコードを返す)。
Step 5(#372)で決めたこと:

- 口は `#[repr(C)]` の表 `JuizPosePort { abi_version, struct_size, ctx, get, set }`(`juiz_sdk::transformations::port`)。
  `get(ctx, reference, out, frame_id, error)` / `set(ctx, reference, pose, out, frame_id, error)` は C の関数で、
  ホスト(juiz_core)が呼び出し元 Container の `ContainerPoseHandle` を包んで作る(`PosePortOwner`)。
  境界を越えるのは C の構造体とバイト列だけで、エラーの種類はコードで運ぶ(プラグインの juiz_sdk とホストの juiz_sdk は
  別の複製で、`JuizError` を downcast できないため)。表の版は 1(並びを変えるときに上げ、後ろに足すときは `struct_size` で見分ける)。
- 口のアドレスを載せる場所:
  - C++・Haskell: 引数の `CapsuleMap` の見えない欄(`CapsuleMap::pose_port`、値・param とは別で、フレームや表示には出ない)。
    ホストは ContainerProcess を呼ぶ間だけ置き、呼び終わったら外す。
  - Rust の C ABI プラグイン: 引数のフレームのヘッダの `pose_port`(数。rust-plugin-abi.md §8.4)。知らないキーは無視されるので
    ABI の版は 1 のまま。グルーは `PortRegistry` をプラグインの中の `ContainerImpl` に呼び出しの間だけつなぐので、
    利用者は Rust ABI と同じく `c.pose("map")` / `c.set_pose(...)` で読み書きする(書き方は §12.1 と同じ)。
- cjuiz(`juiz_sdk.h`)の関数と構造体:

  ```c
  typedef struct juiz_vector3 { double x, y, z; } juiz_vector3;
  typedef struct juiz_quaternion { double x, y, z, w; } juiz_quaternion;     /* (x, y, z, w) */
  typedef struct juiz_pose { juiz_vector3 position; juiz_quaternion orientation; } juiz_pose;
  typedef struct juiz_time { int64_t sec; int32_t nanosec; uint32_t reserved; } juiz_time;
  typedef struct juiz_pose_stamped { juiz_pose pose; juiz_time stamp; } juiz_pose_stamped;

  int64_t juiz_container_pose_get(capsule_map* cm, const char* reference, juiz_pose_stamped* out);
  int64_t juiz_container_pose_set(capsule_map* cm, const char* reference, const juiz_pose* pose, juiz_pose_stamped* out);
  const char* juiz_container_pose_last_error(void);   /* このスレッドの最後のエラー文 */
  ```

  `cm` は entry point が受け取った引数をそのまま渡す。`juiz_pose_stamped` は基準の名前を持たない(基準は渡した `reference`)。
- エラーコード(-9xx、§17 の名前と対応): `JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR`(-901、口が無いときも)、
  `JUIZ_NO_COMMON_ANCHOR_ERROR`(-902)、`JUIZ_POSE_NOT_WRITABLE_ERROR`(-903)、`JUIZ_INVALID_POSE_VALUE_ERROR`(-904)、
  `JUIZ_TRANSFORMATIONS_ERROR`(-905、そのほか)。
- cppjuiz(`juiz/pose.h`): `juiz::pose(container, "map")` / `juiz::set_pose(container, "map", juiz::make_pose(...))` が
  `juiz_pose_stamped` を返し、失敗は `juiz::TransformationsError` の子の例外(`NoCommonAnchorError` など)を投げる。
  ContainerProcess の entry point(`CONTAINER_PROCESS_FACTORY` など)は呼び出しの間 (Container のポインタ, `cm`) をスレッドごとに積み、
  `juiz::pose` は渡された Container のポインタで探す(今呼ばれている Container 以外のオブジェクトは `TransformationsNotAttachedError`)。
  捕まえなかった `TransformationsError` は entry point がそのコードにして返す。辞書にするのは `juiz::pose_to_value`。
  四元数と角度の変換(#412): `juiz::from_rpy(roll, pitch, yaw)` → `juiz_quaternion`、`juiz::to_rpy(q または pose)` →
  `juiz::Rpy{roll, pitch, yaw}`、平面用に `juiz::make_pose_2d(x, y, yaw)`(z = 0)と `juiz::yaw(q または pose)`。
  規則は Rust の `Quaternion::from_rpy` / `to_rpy` と同じ(固定軸 X→Y→Z、q = q_z ⊗ q_y ⊗ q_x)。`to_rpy` は正規化してから変換し、
  長さ 0・NaN・無限大の四元数は (0, 0, 0) にする。
- hsjuiz(`Juiz.Pose`): ContainerProcess の関数は Container の状態だけを受け取るので、`containerPose "map"` /
  `setContainerPose "map" p` は「今呼ばれている ContainerProcess の Container」を読み書きする。エントリポイントが呼び出しの間
  `cm` をスレッドごとの変数(cbits の `hsjuiz_swap_pose_call`)に置く(foreign export の呼び出しは bound thread なので、
  関数の中の同じスレッドから読める)。値は `ContainerPose { cpPosition, cpOrientation }` と `PoseTime`、エラーは `JuizError`
  (コード -901〜-905)。
- 口は呼び出しの間だけ有効。プラグインが別スレッドを立てたり、引数を取っておいて後で読み書きしたりはできない
  (`TransformationsNotAttachedError`)。Pose 通知で同じスレッドから別の ContainerProcess が入れ子に呼ばれても、それぞれの口を使う。
- 古い組み合わせ: C++・Haskell は libjuiz_sdk の版がホストと同じでなければ読み込まれない(#136)ので、口を知らない libjuiz_sdk との
  組み合わせは起きない。古いプラグイン(関数を呼ばない)はそのまま動く。Rust の C ABI プラグインは、古いグルーは `pose_port` を無視し
  (読み書きは今まで通り `TransformationsNotAttachedError`)、新しいグルーは古いホスト(`pose_port` なし)で同じエラーになる。
- 全言語のテストで §3.5 の値を確かめた: `juiz_core/tests/integration/{cpp,haskell,rust}_transformations_test.rs`
  (例は examples/cpp/component/cpp_pose_component、examples/haskell の pose_component_hs、
  juiz_sdk/test_packages/pose_part_component。Rust は C ABI と Rust ABI の両方)。

## 13. API・CLI・管理画面

### 13.1 System の API(CRUD のクラス / 関数)

| 種類 | クラス / 関数 | 引数 | 戻り値 |
|---|---|---|---|
| read | `container/pose` | `identifier`、`reference` | `PoseStamped` |
| update | `container/set_pose` | `identifier`、`reference`、`pose`(`Pose`) | `PoseStamped`(書いた後の値) |
| read | `transform/list` | (なし) | Transform の ID の配列 |
| read | `transform/profile_full` | `identifier` | `{identifier, source, destination, transform: Transform, stamp}` |
| create | `transform/create` | `source`、`destination`、`translation`、`rotation` | profile |
| update | `transform/update` | `identifier`、`translation`、`rotation`(省いたものは今のまま) | profile |
| delete | `transform/destroy` | `identifier` | profile |
| update | `anchor/update` | `identifier` | `{identifier, stamp}` |
| read | `transformations/tree` | `anchor` | `{identifier, pose: Pose, transform: Transform?, children: [...]}` の入れ子 |
| read | `pose_listener/list` | (なし) | ID の配列 |
| create | `pose_listener/create` | §8.3 の項目 | profile |
| delete | `pose_listener/destroy` | `identifier` | profile |

- 削除は既存のクラスに合わせて `destroy` という名前にする。
- Anchor の作成・削除は Container の API(`container/create` の type_name が `periodic_anchor` / `static_anchor`)。
- CRUD・HTTP・ブローカー越し(BrokerProxy)・pyjuiz のクライアントに足す。引数・戻り値は `juiz_std.geometry` の型として登録し、`type/list`・型検査・OpenAPI に載せる。

Step 3(#370)で決めたこと:

- Container・Anchor・Process は ID か名前で指定する。API の名前は**根のネームスペース**から解決する(`robot1/base0`、`base0::mobile_base`、`/robot1/base0` も可、§9.3 の規則)。System を `--namespace robot1` で起動していても、名前にはネームスペースを付ける。
- 引数の渡し方(HTTP では READ / DELETE はクエリ、CREATE / UPDATE はボディ。QMP・IPC は param と引数):

  | クラス / 関数 | 方法 | param(クエリ) | ボディ |
  |---|---|---|---|
  | `container/pose` | READ(GET) | `identifier`、`reference` | |
  | `container/set_pose` | UPDATE(PATCH) | `identifier` | `reference`、`pose` |
  | `transform/list`・`anchor/list`・`pose_listener/list` | READ | | |
  | `transform/profile_full`・`pose_listener/profile_full` | READ | `identifier` | |
  | `transform/create` | CREATE(POST) | | `source`、`destination`、`translation`、`rotation`(`{"__manifest__": {...}}` に包んでもよい) |
  | `transform/update` | UPDATE | `identifier` | `translation`、`rotation`(省いたものは今のまま) |
  | `transform/destroy`・`pose_listener/destroy` | DELETE | `identifier` | |
  | `anchor/update` | UPDATE | `identifier` | |
  | `transformations/tree` | READ | `anchor` | |
  | `pose_listener/create` | CREATE | | §8.3 の項目(`__manifest__` に包んでもよい) |

- 表にない `anchor/list`(Anchor の ID の配列)と `pose_listener/profile_full`(`{identifier, container, process, arg, reference, on_update}`)も足した。
- `reference`・`anchor` のクエリに HTTP の ID(`http://<host>:<port>/container/...`)を渡すと、`identifier` と同じく自分を指すものは core の ID に直す。
- ボディの `pose`・`translation`・`rotation` は `juiz_std.geometry` の `Pose`・`Vector3`・`Quaternion` で検査し、合わなければエラー(データ型の検査と同じ規則)。
- `juiz_std.geometry` の `Pose`・`PoseStamped`・`Transform`・`TransformStamped`(と中の型)は、型の登録所(`type/list`、使う側の名前 `transformations`)と OpenAPI の `components.schemas` にいつも載る。OpenAPI の Transformations の API の応答・ボディはこれらを参照する。そのため `juiz_core` は `juiz_std_types` に依存する。
- Rust は `BrokerProxy` の親に `TransformationsBrokerProxy` を足した(`container_pose`・`container_set_pose`・`transform_*`・`anchor_*`・`transformations_tree`・`pose_listener_*`)。`pose_listener_create` 以外は `&self`(登録所は自分の Mutex を持つ)。ブローカー越しの Transform・Pose 通知の ID(`transform://…`・`pose_listener://…`)は中の core の ID を書き換えずに送る。
- `container_create` は manifest の `namespace` と `rate` も使う(引数に無いとき)。Rust・ブローカー越しに Periodic Anchor の rate を指定できる。
- pyjuiz の `SystemProxy` に `create_anchor(name, kind='static', rate=None, namespace=None)`・`pose`・`set_pose`・`transforms`・`transform`・`create_transform`・`update_transform`・`destroy_transform`・`anchors`・`update_anchor`・`tree`・`pose_listeners`・`pose_listener`・`create_pose_listener`・`destroy_pose_listener` を、`ContainerProxy` に `pose(reference)`・`set_pose(reference, pose)` を足した。値は dict(`juiz_std.geometry` のオブジェクトも渡せる)、`translation`・`rotation` はタプルでもよい。クエリの値は URL 用に符号化する。

### 13.2 CLI

```text
juiz container pose <id> --reference map                      # 読む
juiz container pose <id> --reference map --set '<Pose の JSON>' # 書く
juiz transform list [--long]
juiz transform create <source> <destination> [--translation x,y,z] [--rotation x,y,z,w]
juiz transform update <id> [--translation ...] [--rotation ...]
juiz transform delete <id>
juiz anchor update <name>
juiz anchor tree <name>
juiz list --long        # Anchor の種類と rate を表示
```

Step 3(#370)で決めたこと:

- 実際のコマンドは次のとおり。サーバーは `-s`(既定 `http://127.0.0.1:8000`)。

  ```text
  juiz container pose <id> --reference <基準> [--set '<Pose の JSON>']   # -r でもよい。PoseStamped を JSON で出す
  juiz transform list [--long]                     # --long は「始点 -> 終点  translation (…)  rotation (…)」
  juiz transform info <id | 始点 終点>
  juiz transform create <始点> <終点> [--translation x,y,z] [--rotation x,y,z,w]
  juiz transform update <id | 始点 終点> [--translation x,y,z] [--rotation x,y,z,w]
  juiz transform delete <id | 始点 終点>             # destroy でもよい
  juiz anchor list
  juiz anchor update <name>
  juiz anchor tree <name> [--json]                 # 字下げの木。各行は名前と Anchor から見た位置・向き
  juiz container list --long                       # Anchor の行の下に「anchor: periodic (10 Hz)」「anchor: static」
  ```

- Transform は ID が長いので、始点と終点の 2 つ(ID か名前)でも指せる。CLI が一覧から探す。
- 一覧のコマンドは `juiz container list` なので、`juiz list --long` は `juiz container list --long` にした。
- Pose 通知の CLI は作っていない(conf・API・pyjuiz で登録する)。

### 13.3 管理画面

Anchor ごとの木と、各 Container の Pose の数値を表示する。3D 表示はしない(§15)。Step 6(#373)。System をまたぐ Transform の表示は §19.10・§19.14(#416)。

Step 6(#373)で決めたこと:

- 画面は自動更新のたびに `transform/list` と、Anchor の容器(profile に `anchor` があるもの)ごとの `transformations/tree?anchor=<ID>` を取る。`transform/list` が無い古い juiz では Transformations を出さない。対象は最上位の系だけ(システムをまたぐ Transform は対象外なので、副系の容器には出さない)。
- ツリー: Anchor の容器に「基点」の印。系の下の分類「変換木」に、Anchor ごとの木を入れ子で出す。Anchor が別の Anchor の子なら、親の木の途中にも、自分を根とする木にも出る。
- 性質欄(容器): Anchor なら種類(周期更新 / 静的)・周期と木(各ノードに Anchor から見た位置)。どの容器にも「位置姿勢」(基準を選ぶ。候補はその容器が入っている木の容器、既定は容器が子として入っている木の Anchor)と「変換」(親から来る・子へ出る Transform と値)。
- 値は位置 (x, y, z)、四元数 (x, y, z, w)、回転角(ロール・ピッチ・ヨー [度]、z → y → x の順の回転)を小数点以下 3 桁で出す。基準が木の Anchor なら木の値をそのまま使い、それ以外は `container/pose` で取る。木に入っていない Transform の値は `transform/profile_full` で取る。
- 用語(#111 の漢字を正とする表記): Transformations = 変換木、Anchor = 基点、Transform = 変換、ContainerPose = 位置姿勢、基準 Container = 基準。

## 14. readonly ContainerProcess

readonly の ContainerProcess(#377)は、Container を**読み出しロック**で呼ぶ ContainerProcess。同じ Container の readonly 同士は並行に走る。中から ContainerPose は読めるが、書き込みはエラー(`PoseNotWritableError`)になる。Container の利用者の状態も書き換えない約束で、Rust と C++ は型で止める。

### 14.1 manifest

- ProcessManifest に `readonly`(真偽値、既定 `false`)を足した。`false` のときは JSON に出さない。
- `readonly` は Factory の manifest で決まる(関数の書き方で決まる)。conf のインスタンスの manifest では変えられない。
- ProcessProfile にも `readonly` が載る(`false` なら出さない)。API・CLI・管理画面の profile で見える。
  - CLI: `juiz container-process list --long`(と `process list --long`)は readonly の行の下に `readonly` の 1 行を出す。`container-process info` も profile の後に同じ行を出す。
  - 管理画面: ツリーの容器処理の行に「読取」の印(破線の枠)、性質欄の基本に「読み取り専用 readonly」(する/しない)。
- readonly でない ContainerProcess は今まで通り書き込みロックで呼ぶ。readonly と readonly でないものは並行に走らない(書き込みロックを待つ)。

### 14.2 ロックと書き込みの止め方

- ホストは manifest の `readonly` を見て、Container を `lock()`(読み出し)で取る。readonly でなければ `lock_mut()`。
- 呼び出しの間、ホストは呼んだスレッドに「この Container の readonly を呼び出し中」を覚える(`juiz_sdk::transformations::ReadonlyScope`)。その間そのスレッドから同じ Container の `ContainerPoseHandle::set_pose` を呼ぶと `PoseNotWritableError`(文は `the pose can not be written from a readonly container process`)。スレッドごと・Container ごとなので、同じ Container の readonly が走っている間も、API・CLI・別の Process・Pose 通知からの書き込みは今まで通りできる。
- C ABI のプラグイン(C++・Haskell・Rust C ABI)には書き込みの口が `PoseNotWritableError`(`JUIZ_POSE_NOT_WRITABLE_ERROR`)を返す口を渡す(`PosePortOwner::new_readonly`)。
- Composite の ContainerProcess は子がそれぞれ自分のロックを取るので、readonly の子は読み出しロックで、そうでない子は書き込みロックで呼ばれる。Composite の manifest の `readonly` は付けない(`false`)。

### 14.3 言語ごとの書き方

| 言語 | 書き方 | 利用者の状態の書き換え |
|---|---|---|
| Rust | 第 1 引数を `&ContainerImpl<T>`(`mut` なし)にすると、マクロが readonly にする。属性に `readonly = true` を書いてもよい(`&mut` のままだとコンパイルエラー)。`T: Sync` が要る | 型で止める(`&self` なので `set_pose` も呼べない) |
| Python | `@juiz_container_process(container_type="...", readonly=True)` | 止められない(書き手が守る)。`juiz.set_pose` は `PoseNotWritableError` |
| C++ | manifest に `.readonly()`、関数は Container を `const T*` で受け取る | `const T*` で受ければ型で止まる(`T*` のままでも動くが、書き換えないのは書き手が守る)。`juiz::set_pose` に const の Container を渡すと `static_assert` で止まる |
| Haskell | manifest に `& readonly True` | 止められない(書き手が守る)。`setContainerPose` は `juizPoseNotWritableError` |

```rust
#[juiz_container_process(container_type = "mobile_base")]
fn mobile_base_where(c: &ContainerImpl<MobileBase>, reference: String) -> JuizResult<Capsule> {
    Ok(c.pose(&reference)?.into())
}
```

### 14.4 Rust のプラグインの形

- Rust ABI(`abi: rust`): `juiz_sdk::factory::container_process_factory_readonly(manifest, f)`。マクロが使う。読み出しロックを取るのはプラグインの中のグルー。
- C ABI: 入口の表は今まで通り(kind 3、`container_call`)。manifest の `readonly` を見てホストが読み出しロックで呼び、プラグインのグルー `call_container_process_readonly` が handle を `&ContainerImpl<T>` に戻す。ABI の版は変えない(古いプラグインは `readonly` を出さないので今まで通り)。
- C ABI では同じ Container の呼び出しが並行するので、プラグインの中の Container に口をつなぐのは最初の呼び出し、外すのは最後の呼び出しが終わったとき(`SharedPortAttachment`)。読み書きは呼んだスレッドの口を使い、別のスレッドからは `TransformationsNotAttachedError`。

## 15. 範囲の外

- System をまたぐ Transform のうち §19.11 に挙げたもの(親からの push、System 間の時計合わせなど)。System をまたぐ Transform そのものは §19 で扱う。
- 過去の時刻の位置姿勢の問い合わせ(tf2 の時刻つきバッファ)。
- 管理画面の 3D 表示。
- Transform の自動作成(どの木にも入っていない Container への `set_pose` はエラー)。

## 16. 他の機能との関係

- **Composition**: CompositeContainer も Container なので自分の Pose を持つ。スロットの Container はそれぞれ自分の Pose のまま。
- **ネームスペース**: Anchor も Container なのでネームスペースで分かれる。複製したロボットごとに `map` を別にできる。
- **データ型**: 値は `juiz_std.geometry` の型。`Transform` / `TransformStamped` はこの Step で `juiz_std/geometry.jidl` に足した(datatype.md §24、§25.2)。
- **.juizapp**: conf に `transforms:` / `pose_listeners:` が入るだけで、形式の変更はない。

## 17. エラー

| エラー | いつ |
|---|---|
| `InvalidPoseValueError` | 長さ 0 の四元数、NaN・無限大 |
| `NoCommonAnchorError` | R と C の両方を含む Anchor がない(読み・書き) |
| `PoseNotWritableError` | 根の Anchor・基準自身・基準が子孫の Container への書き込み、readonly の ContainerProcess からの書き込み(§14) |
| `TransformAlreadyExistsError` | 同じ (source, destination) の Transform |
| `TransformTreeError` | §5.2 の木の規則を破る変更 |
| `TransformNotFoundError` | 指定の Transform がない |
| `PoseListenerError` | Pose 通知の登録の不備(引数・型・二重登録) |
| `CrossSystemTransformError` | System をまたぐ Transform の不備(§19.2: 終点が別 System なのに回す先の System につながっていない、両端とも別 System、`source_system` が分からない、始点が相手の System の Container でない) |

Step 1 で上の名前のまま `JuizError` に足した。あわせて `TransformManifestInvalidError`(`transforms:` の 1 項目の不備)と `TransformationsNotAttachedError`(登録所につながっていない Container の読み書き、§9.1)を足した。

## 18. Step と担当する節

| Step | issue | 節 |
|---|---|---|
| 0 仕様書・`juiz_std.geometry.Transform` | #367 | 全体、§3.1 の型 |
| 1 SDK | #368 | §3、§9.1、§12.1、manifest |
| 2 core | #369 | §4〜§11(決めたことは §9.3) |
| 3 API・CLI | #370 | §13.1、§13.2(決めたことは各節の「Step 3 で決めたこと」) |
| 4 Python | #371 | §12.2 |
| 5 C ABI | #372 | §12.3 |
| 6 管理画面 | #373 | §13.3 |
| 7 サンプル 4 言語 | #374 | §3.5 の値、§11 の形 |
| 11 readonly ContainerProcess | #377 | §14 |

System をまたぐ Transform(#376、§19):

| Step | issue | 節 |
|---|---|---|
| 0 仕様書 | #413 | §19 全体 |
| 1 core | #414 | §19.2〜§19.7、§19.9(決めたことは §19.12) |
| 2 API・CLI・pyjuiz | #415 | §19.8、§19.4 の API、HTTP / QMP / IPC 越しのテスト(2 プロセス) |
| 3 管理画面 | #416 | §19.10 |
| 4 サンプル 4 言語・サイトのガイド | #417、OpenJUIZ.github.io#69 | §19.1 の例 |
| 5 リリース | #418 | – |

## 19. System をまたぐ Transform

Step 0(#413)で足した節。親 issue #376。「(Step n で決める)」と書いた所は、その Step の PR でこの節に書き足す。

### 19.1 使い方の例

- **例 A(親子の System)**: 地図の System S1 に `map`(Anchor)、ロボットの System S2(S1 のサブシステム、または別プロセス)に `odom → base0 → camera0`。S2 に `map → odom`(始点は S1 の `map`)を張ると、S2 の中で `camera0` を `map` から見た位置が読め、S1 からも S2 の `camera0` を `map` から見て読める。
- **例 B(複数のロボット)**: S1 の `world` から S2 の `robot1/odom`、S3 の `robot2/odom` に張る。S2 と S3 はどちらも `world` の写しを持つので、それぞれで `world` から見た位置を読んで合成すれば、robot1 の camera から見た robot2 の base が分かる(§19.8)。

用語:

| 用語 | 意味 |
|---|---|
| **またぐ Transform** | 始点 P が別 System の Container、終点 C が自分の System の Container である Transform |
| **写しの始点** P' | 自分の System の登録所の中で P の代わりになるノード。Container ではない(`container/list` に出ない) |
| **写しの Anchor** B' | P の PoseState にある Anchor B ごとに、自分の System の登録所に置く Anchor。Container ではない |
| **相手の System** | P を持つ System(親)。こちらが写しを取りに行く先 |

### 19.2 持ち主と端点

1. またぐ Transform は**終点(子)の System** が持つ。木の規則(§5.2)で各 Container の親はちょうど 1 つなので、Transform は子に属する情報である。`set_pose(C, R, p)` は親 → C の Transform を書き換える(§7.2)ので、子の System が持てば `set_pose` は今どおり C の System の中だけで終わり、ブローカー越しの書き込みやロックは要らない。
2. 始点 `source` は別 System の Container を**ブローカー付きの ID** で書く(`http://host:8000/container/map::periodic_anchor`、`qmp://…`、`ipc://…` など、`container/pose_state`(§19.4)を呼べるブローカーなら何でもよい)。名前では書けない(名前は自分の System の中で解決する)。終点 `destination` はいつも自分の System の Container(名前か ID)。`.juizapp` のアプリで依存する基礎の Container を書くときは `package://<名前>/<ID>` と書け、`juiz run` が app-package.md §10.4 の規則でつなぎ先のブローカーに置き換える(#425)。
3. 始点の Container は相手の System に登録された Container(Anchor を含む)でなければならない。相手の System にとっても別 System の Container(相手の `ContainerProxy`)なら、相手が `container/pose_state` をエラーで返す。
4. Transform の ID は今と同じ `transform://<source の ID>|<destination の ID>`。source はブローカー付きの ID のまま入る。
5. **親の System で作ろうとしたとき**(conf の `transforms:` や `transform/create` で、始点が自分の Container で終点が別 System の Container): 終点の System の `transform/create` に回して作る。引数の `source` は自分の core の ID、`source_system` に自分の System の UUID を付ける。回された System は、`source_system` の UUID を自分がつないでいる System(マスターシステム・サブシステム)から探し、その BrokerProxy で source を書き換えて作る。こちらには何も残さない。戻り値は相手の profile。
6. 回せないとき(終点の System につながっていない、相手が `source_system` を知らない)、両端とも別 System のときは `CrossSystemTransformError`。
7. `transform/update`・`transform/destroy`・`transform/profile_full` も、ID の終点が別 System なら終点の System に回す。
8. conf で回す Transform は、conf の `subsystems:` / マスターシステムとのつなぎが済んだ後に作る。失敗は §11 の conf の読み込みエラーと同じ扱い。

### 19.3 写しの Anchor と木

子の System S2 は、またぐ Transform P → C ごとに P の PoseState(Anchor B ごとの pose_B(P)、§6.1)を取り込み(§19.4)、自分の登録所に置く:

- **写しの始点 P'**: P の代わりのノード。P' → C が S2 の木の Transform になる。同じ P を始点にする Transform が S2 に複数あっても P' は 1 つ。
- **写しの Anchor B'**: P の PoseState の項目 B ごとに 1 つ。B' の木の直下の子は、PoseState に B の項目を持つすべての P' で、`pose_B'(P') = pose_B(P)`(取り込んだ値)。B' の木はそこから先、S2 の Transform を今どおりたどる。P 自身が Anchor なら(`pose_P(P)` = 単位なので)P' がそのまま P の写しの Anchor になる。
- B' の木の値は §3.4 の計算規則のまま(根の直下の値が単位でないだけ)。S2 の Container の PoseState に B' の項目が入るので、`pose(camera0, map)` は今の規則(§7.1)のまま答えられ、S2 からさらに取り込む S3 には B' の項目も渡る(多段のつながり)。
- 木の規則(§5.2)は B' の木にもそのまま当てはめる。取り込みで B' の木が規則を破るとき(2 つの P' から同じ Container に届くなど)は、その B' を作らず WARN を 1 回出す。木の形が変わったら検査し直す。
  - 取り込みは P' ごとに別々に当てはまるので、一部の P' だけを当てはめた間も B' が一瞬でも作られないようにする(#437)。検査では、最後に取れた PoseState に B があった P' を、まだ当てはめていなくても B' の子として数える。また、取り込みを 1 度も終えていない(成功も失敗もしていない)P' があるうちは新しい B' を作らない。
- 写しの値の更新は StaticAnchor と同じく、取り込んだ値が変わったときに登録所のロックの下で計算する(§6.2)。木の stamp は**取り込んだ値の stamp**(相手の System の時計の値)のまま。
- P'・B' への `set_pose` と、P' → C 以外の P'・B' を端点にする Transform の作成は `PoseNotWritableError` / `CrossSystemTransformError`。C 以下の `set_pose` は §7.2 のまま(P' → C の Transform を書き換えてよい)。
- **親の System S1 は子の木を取り込まない**。S1 の登録所に S2 の Container は入らず、木の形は各 System の中で閉じる。S1 から S2 の Container の位置を読むのは §19.8 の回し方で行う。

### 19.4 取り込み(子が親に取りに行く)と `container/pose_state`

- S2 は始点 P ごとに取り込みのスレッドを持ち、`1 / rate` 秒ごとに相手の System の `container/pose_state` を 1 回呼ぶ。
- `rate`(Hz、正の数、既定 10.0)と `timeout`(秒、正の数、既定 3.0)は Transform ごとに conf・`transform/create` で指定する。同じ P を始点にする Transform が複数あれば、いちばん大きい `rate` といちばん小さい `timeout` を使う。またぐ Transform でない Transform に書くと WARN を出して使わない。
- 取り込みのスレッドは Periodic Anchor と同じく、`System::setup` の最後で始め、起動後に作ったものはすぐ始める。始めてすぐ 1 回目を取りに行く。Transform の作成は 1 回目を待たずに戻る(1 回目が届くまでは B' がないので `NoCommonAnchorError`)。
- 通信は登録所のロックの外で行い、値を当てはめるときだけ登録所のロックを取る(§9.2 の順番は変えない)。
- 1 回の取り込みが周期より長くかかったときは、次をすぐ始める(§4.3 と同じ)。

**`container/pose_state`**(READ、相手の System が持つ API):

| param(クエリ) | 内容 |
|---|---|
| `identifier` | P の ID か名前(必須) |
| `since` | 前回受け取った `version`。同じなら `entries` を省く(省略可) |
| `system` | 前回受け取った `system`(相手の UUID)。今の UUID と違えば `since` を無視する(相手が起動し直して版番号が戻ったときのため) |
| `subscriber` | 取りに来た System の UUID(省略可、§19.10 の表示に使う) |
| `destination` | 取りに来た System での終点の ID。複数なら繰り返す(省略可、§19.10) |
| `timeout` | 取りに来た側の `timeout`(省略可、§19.10) |

戻り値:

```json
{
  "identifier": "core://core/container/base_a::mobile_base",
  "system": "<相手の System の UUID>",
  "version": 42,
  "changed": true,
  "entries": [
    {
      "anchor": "core://core/container/map::periodic_anchor",
      "origin_system": "<Anchor を本当に持つ System の UUID>",
      "origin_anchor": "core://core/container/map::periodic_anchor",
      "pose": { "position": {"x": 1, "y": 0, "z": 0}, "orientation": {"x": 0, "y": 0, "z": 0.7071067811865476, "w": 0.7071067811865476} },
      "stamp": { "sec": 1762171200, "nanosec": 0 },
      "version": 41
    }
  ]
}
```

- `version` は P の PoseState の版番号(§9.1、書くたびに増える)。`changed` が `false` なら `entries` は空で、子は計算も通知もしない。
- `pose` は `juiz_std.geometry.Pose`、`stamp` は `juiz_std.common.Time`。
- `anchor` は相手の System での Anchor の ID。相手が写しとして持つ Anchor(多段)なら相手の写しの ID(ブローカー付き)。`origin_system` / `origin_anchor` は、その Anchor を本当に持つ System の UUID と、その System での core の ID(多段でも変えずに運ぶ)。
- 量は Anchor 1 つにつき Pose 1 個(JSON で 200 バイト程度)。10 Hz・Anchor 数個なら数 KB/s。
- P が見つからない、P が相手にとっても別 System の Container のときはエラー(子からは取り込みの失敗に見える、§19.6)。
- この API はまたぐ Transform 以外からも使ってよい(Container の PoseState を見るだけの読み出し)。

### 19.5 名前と ID

- **写しの Anchor の ID**: `entries[].anchor` が core の ID(相手の System の Anchor)なら、P の ID のブローカーの部分で書き換えたもの(`core://core/container/map::periodic_anchor` を `http://host:8000/container/...` から取ったなら `http://host:8000/container/map::periodic_anchor`)。ブローカー付き(相手の写し、多段)なら相手の ID の文字列のまま。
- 写しの Anchor は (`origin_system`, `origin_anchor`) で同じものとみなす。別の P'・別の経路から同じ組が来たら同じ B'(直下の子の P' が増える)。ID は最初に取り込んだときのものを使い、B' が消えるまで変えない。
- **写しの始点 P' の ID** は Transform の source の ID(ブローカー付き)。
- **基準・Anchor の指定**(`pose`・`set_pose` の reference、`transformations/tree` の anchor など):
  1. 自分の System の Container の名前・ID に当たれば、それを使う(自分が優先)。
  2. 当たらなければ、写しの Anchor・写しの始点を探す。ID はブローカー付きの ID、または `origin_anchor`(core の ID)で探す。名前は `origin_anchor` の名前(ネームスペースつき、§5.4)で探す。
  3. 2 で複数当たれば、名前の解決と同じくエラー。
  4. API に `reference_system`(UUID)が付いていれば、2 は `origin_system` がその UUID のものだけを探し、1 を飛ばす(§19.8 の回し方で使う)。
- 自分の System に同じ名前があるときに写しを指すには、写しの ID で指定する。
- `PoseStamped.header.frame_id` は、基準が写しのときは写しの ID(ブローカー付き)。
- **循環**: `entries` のうち `origin_system` が自分の UUID のもの(S1 → S2 → S1 と回ってきた自分の Anchor の値)は取り込まない。

### 19.6 つながらなくなったとき

- 取り込みに失敗しても(相手が落ちた、P が消えた、応答がエラー)、最後に取れた値を stamp つきのまま持ち続ける。読む側は stamp で古さが分かる。
- ログ: 最初の失敗で WARN を 1 回、以降は DEBUG。つながり直したら INFO を 1 回(#301・#276 の ERROR 連発の反省)。
- 失敗している間も取りに行き続けるが、間隔は `1 / rate` 秒と 1 秒の長い方にする。1 回の呼び出しは `timeout` を超えて待たない。
- 最後に取れてから `timeout` 秒たっても取れなければ、その P' からの写し(B' の直下の子としての P'、ほかに子がなくなった B')を S2 のすべての PoseState から消す(§10 の Anchor の削除と同じ)。以後その基準で読むと `NoCommonAnchorError` で、理由に「相手の System(<ID>)につながらない」を入れる。**Transform は消さない**。
- つながり直せば、次の取り込みで写しが自動で戻る(`since` は使わず全部取る)。
- #303 の `system/remove_subsystem` で相手の System が外れたときも、その System を始点にする写しを消す(Transform は残し、取り込みは続ける)。
- 終点 C を消せば Transform も消え(§10)、P を始点にする Transform がなくなった P' の取り込みも止める。

### 19.7 Pose 通知

- きっかけ(§8.1)は変えない。写しの取り込みは明示的な書き込みではないので、`on_update: true` の Pose 通知だけを呼ぶ(Anchor の更新と同じ扱い)。
- S2 の中の `set_pose`・`transform/update`(P' → C の Transform を含む)は今どおり通知を出す。
- S1 で P を `set_pose` しても、S2 の通知(`on_update: false`)は呼ばない。

### 19.8 別 System の Container の位置を読む・書く

- S1 の `container/pose`・`container/set_pose` に、`identifier` として別 System の Container(ブローカー付きの ID)を渡すと、その System の同じ API に回す。基準 R が S1 の Container なら、引数の `reference` は R の core の ID、`reference_system` に S1 の UUID を付ける。相手は §19.5 の 4 で R の写しを探す(例 A: S1 から S2 の `camera0` を `map` から見て読む)。
- 基準 R が別 System で、C が自分の Container のときは、R の System に `pose(R, C)`(C は core の ID と自分の UUID)を頼み、逆(§3.3)にして返す。`set_pose` はこの形では回さない(`CrossSystemTransformError`。書き込みは C を持つ System で、C の基準を指定して行う)。
- 回した先で写しが見つからなければ `NoCommonAnchorError`。基準に使えるのは、回した先が写しとして持つ Container(写しの Anchor・写しの始点)だけで、相手の System のほかの Container は基準にできない。
- 例 B のように子同士(S2 と S3)で読むときも同じ。S2 から S3 の `robot2/base` を S2 の `robot1/camera` から見て読みたければ、S2 で `pose(robot1/camera, world)`、S3 で `pose(robot2/base, world)` を取り、§3.4 で合成する(共通の写しの Anchor `world` を介する)。この合成を API でまとめるかは Step 2(#415)で決める。
- Rust の `ContainerProxy`・pyjuiz の `ContainerProxy.pose` も同じ規則で回す。

### 19.9 変えないもの

- Python・C ABI・Haskell の `pose` / `set_pose` の形は変えない。基準に写しの Anchor の名前や ID を指定できるようになるだけ。
- ロックの順番(§9.2)・Anchor の更新の手順(§6.2)・Pose 通知のきっかけ(§8.1)は変えない。
- conf の形は、`transforms:` の項目に `rate` / `timeout` が増えるだけ。

```yaml
# S2(子、ロボット)の conf
transforms:
  - source: http://127.0.0.1:8000/container/map::periodic_anchor   # S1 の map
    destination: odom
    rate: 10.0       # 省略時 10.0
    timeout: 3.0     # 省略時 3.0
  - source: odom
    destination: base0
```

```yaml
# S1(親、地図)の conf で書いてもよい。S2 の transform/create に回る(§19.2 の 5)
subsystems:
  - ...                                                             # S2 とつないでおく
transforms:
  - source: map
    destination: http://127.0.0.1:8001/container/odom::static_anchor
```

### 19.10 API・CLI・管理画面の差分

API(§13.1 への追加。細かい形は Step 2、#415 で決める):

| 種類 | クラス / 関数 | 追加 |
|---|---|---|
| read | `container/pose_state` | 新しい API(§19.4) |
| create | `transform/create` | 引数 `rate`・`timeout`・`source_system`。終点が別 System なら回す(§19.2) |
| read | `transform/profile_full` | またぐ Transform に `remote: {system, rate, timeout, state, last_success}` を足す。`state` は `ok`(取れている)/ `stale`(失敗中、`timeout` 前)/ `lost`(写しを消した) |
| read | `container/pose` / update `container/set_pose` | 引数 `reference_system`。`identifier` が別 System なら回す(§19.8) |
| read | `anchor/list` | 写しの Anchor の ID も入れる |
| read | `transformations/tree` | `anchor` に写しの Anchor・写しの始点を指定できる |
| update | `anchor/update` | 写しの Anchor を指定したら、その写しの始点すべての取り込みをその場で 1 回行う(明示要求、§4.4 の 3 と同じ位置づけ) |
| read | `container/profile_full` | 写しを取りに来ている System があれば `remote_children: [{system, destinations, last_access}]`。最後の `container/pose_state` から、そのとき渡された `timeout`(なければ 3.0 秒)を過ぎたら消す |

CLI(§13.2 への追加): `juiz transform create` に `--rate`・`--timeout`、`juiz transform list --long` でまたぐ Transform に `(remote: ok | stale | lost)`、`juiz container pose-state <id>`。名前は Step 2 で決める。

管理画面(§13.3 への追加、Step 3、#416):

- 子の System: 写しの Anchor・写しの始点を、Anchor の木の中に「写し」の印で出す(Container ではないので容器の一覧には出さない)。またぐ Transform に状態(`ok` / `stale` / `lost`)を出す。
- 親の System: 子の木は出さない。`remote_children` がある容器に「別 System の子あり」の印だけを出す。
- 副系の容器への表示(§13.3 の「最上位の系だけ」)を広げるかは Step 3 で決める(§19.14 で、写しのある基点から見た位置姿勢だけを出すことにした)。

### 19.11 範囲の外

- 親が子へ変化のたびに送る push(Topic の転送と同じ形)。最初は pull だけにし、遅れが問題になったら足す。
- System 間の時計合わせ。stamp は相手の System の時計の値をそのまま持つ。
- 過去の時刻の問い合わせ(§15 と同じ)。
- 親の System で子の木全体を組むこと(管理画面の印だけ)。
- 相手の System の、写しになっていない Container を基準にすること(§19.8)。

### 19.12 Step 1(#414)で決めたこと

core(`juiz_core::transformations`)の実装で決めたこと。

- **取り込みの辺**: 写しの Anchor B' → 写しの始点 P' は、登録所の中だけの Transform(値は取り込んだ pose_B(P))として持つ。`transform/list` に出さず、`transform/update`・`transform/destroy`・`transform/profile_full` には `TransformNotFoundError`。P' = B'(P 自身が Anchor)のときは辺を置かず、P' がそのまま木の根になる。
- **写しの Anchor の更新**: StaticAnchor と同じ扱い(§4.4 の 2)。取り込んだ値が変わったときに加えて、S2 の中で B' の木の Transform を書き換えたとき(`set_pose`・`transform/update`)も更新する。
- **時刻**: B' の直下の P' の値の stamp は取り込んだ項目の stamp、その先は親の値の stamp をそのまま運ぶ(§19.3 の「木の stamp は取り込んだ値の stamp」)。
- **取り込みの呼び出し**: 写しの始点ごとのスレッドが、1 回ごとに呼び出し用の短いスレッドを立てて `container/pose_state` を呼び、`timeout` 秒待って返らなければ失敗として扱う(返らない呼び出しは待たずに捨てる)。前の呼び出しが返るか見切るまでは次を呼ばない。System の終了は呼び出しの最中でも待たない。
- **呼び出しの口**: `Transformations::set_pose_state_fetcher`(trait `PoseStateFetcher`)。System は CoreBroker を作るときに、ID のブローカー(`http` / `qmp` / `ipc` …)の BrokerProxy で `TransformationsBrokerProxy::container_pose_state` を呼ぶものを置く。BrokerProxy がなければ作る。CoreBroker の読み出しのロックは BrokerProxy を探す間だけ持つ。
- **`anchor/update` に写しの Anchor を渡したとき**: その写しの始点の取り込みスレッドを起こして、すぐ 1 回取りに行かせる。呼び出しは取り込みを待たずに戻る(API の処理の中で相手の System を呼ぶと CoreBroker のロックを持ったまま通信することになるため)。
- **1 回も取れていないとき**: 作ってから `timeout` 秒の間は `stale`、過ぎたら `lost`。その間に P' や写しの名前で読むと `NoCommonAnchorError` で、理由に「まだ取り込んでいない」「相手の System につながらない」「remove_subsystem で外れた」のどれかを入れる。
- **名前の解決**: Transform の端点(作成)は自分の System の Container だけから解決する。写しに当たる名前・ID を端点に書くと `CrossSystemTransformError`(始点はブローカー付きの ID で書く)。終点が別 System の Container のときも、Step 2 で回すようにするまでは `CrossSystemTransformError`。
- **rate / timeout**: 正の有限の数でなければ `TransformManifestInvalidError`。またぐ Transform でない Transform に書くと WARN を出して使わない。
- **API**: `container/pose_state` の core の処理と、CRUD・メッセンジャー系の BrokerProxy・コールバック(param `identifier`・`since`・`system`)は Step 1 で足した。HTTP の経路と OpenAPI、CLI、pyjuiz、`subscriber`・`destination`・`timeout` の引数と `remote_children`、`reference_system` と別 System の Container の `pose` / `set_pose` の回し方(§19.8)、`transform/create` の終点の System への転送(§19.2 の 5・7)は Step 2(#415)。
- `transform/profile_full` の `remote: {system, rate, timeout, state, last_success}` は Step 1 で足した(`rate`・`timeout` はその Transform に書いた値、`system` は 1 回目が届くまで `null`)。

### 19.13 Step 2(#415)で決めたこと

API・CLI・pyjuiz で決めたこと。

- **`container/pose_state` の param**: HTTP は `GET /api/container/pose_state?identifier=…&since=…&system=…&subscriber=…&destination=…&timeout=…`。`destination` は**複数なら `,` でつなぐ**(繰り返しにしない。HTTP のクエリの読み取りが 1 つの名前に 1 つの値のため。メッセンジャー系も同じ形)。取り込みのスレッドは毎回 `subscriber`(自分の UUID)・`destination`(その写しの始点を始点にする Transform の終点の ID)・`timeout` を付ける。
- **`remote_children`**: `container/profile_full` に `[{system, destinations, last_access}]`(`last_access` は `juiz_std.common.Time`)。`subscriber` の付いた `container/pose_state` だけを覚え、そのとき渡された `timeout`(なければ 3.0 秒)の間に次が来なければ消す(読み出しのときに消す)。Container を消せば一緒に消える。CLI の `container list --long` は `remote child: <UUID> -> <終点>, …` の行を出す。
- **`transform/create` の転送**(§19.2 の 5): `TransformManifest` に `source_system`(省略可)を足した。転送する側は、始点を自分の core の ID に、終点を相手での core の ID に直し、`source_system` に自分の UUID を入れて終点の System の BrokerProxy で `transform/create` を呼ぶ。受けた側は、マスターシステム・サブシステムの中からその UUID の System を探し、その BrokerProxy の(種類, 名前)で始点を書き換えて作る(`127.0.0.1:8000` につないでいれば `http://127.0.0.1:8000/container/map::static_anchor`)。戻り値は相手の profile(ID は相手の System での形)。
- **`transform/profile_full`・`update`・`destroy` の転送**(§19.2 の 7): 親の System では Transform を `transform://<自分の core の ID>|<相手のブローカー付きの ID>` で指す。この ID の終点が別 System なら、`transform://<自分の core の ID>|<相手での core の ID>` と param `source_system`(自分の UUID)を付けて回す。受けた側は始点を作成と同じ規則で書き換えてから探す。
- **回せないとき**: BrokerProxy が作れない・相手に届かない(juiz のエラーでない)ときは `CrossSystemTransformError`。相手が juiz のエラー(木の規則を破る、`source_system` を知らないなど)を返したときは、そのエラーに「どの System で起きたか」を足して返す。
- **`container/pose`・`set_pose` の回し方**(§19.8): param `reference_system` を足した。自分の System の Container・写しに当たる名前や ID は自分で計算する(§19.5 の「自分が優先」)。当たらない `identifier` がブローカー付きの ID なら、その System に回す。答えの `header.frame_id` は自分の System での基準の名前(§5.4)に直す。当たらない `reference` がブローカー付きの ID で `identifier` が自分の Container なら、R の System に `pose(R, C)` を頼んで逆にし、`frame_id` は R のブローカー付きの ID。`set_pose` のこの形は `CrossSystemTransformError`。
- **HTTP の BrokerProxy**: `container/pose`・`container/set_pose` の `identifier` は core の ID に書き換えずに送る(別 System の Container を指せるように)。サーバーは自分を指す ID を今どおり core の ID に直す(`localize_identifier`)。
- **例 B の合成**(§19.8): API にはまとめない。それぞれの System で共通の写しの Anchor から見た位置を読み、使う側で合成する。
- **conf**(§19.2 の 8): `transforms:` のうち終点がブローカー付きの、別 System の Container の項目は、`subsystems:` / `mastersystem:` の後で回す。ほかの項目は今どおり `composite_containers:` の後。
- **CLI**: `juiz transform create … --rate <Hz> --timeout <秒>`、`juiz transform list --long` はまたぐ Transform に `  (remote: ok | stale | lost)` を足す、`juiz container pose-state <id>`(PoseState を JSON で出す)。`juiz container pose` に別 System の Container の ID を渡すと回して読む。
- **pyjuiz**: `SystemProxy.create_transform(…, rate=, timeout=)`、`SystemProxy.pose_state(container)`・`ContainerProxy.pose_state()`。`pose`・`set_pose`・`create_transform` に**別の SystemProxy の ObjectProxy** を渡すと、その System へのブローカー付きの ID(`ObjectProxy.broker_identifier()`、`http://host:port/container/…`)にして送る。
- **残したもの**: Rust の Process の中から、別 System の Container を指す `ContainerProxy` の `pose` を呼んだときの回し方(登録所から CoreBroker を呼ぶ口が要る)。管理画面(Step 3、#416)。

### 19.14 Step 3(#416)で決めたこと

管理画面(§13.3)で決めたこと。

- **取得**: 自動更新のたびに、§13.3 の取得に加えて `anchor/list` を取り、最上位の系の容器でない ID(写しの Anchor)ごとに `transformations/tree` を取る。始点が最上位の系の容器でない Transform(またぐ Transform)ごとに `transform/profile_full` を取り、`remote` を使う。
- **`system/profile_full` の `remote_children`**: 管理画面が容器ごとに `container/profile_full` を呼ばずに済むように、`system/profile_full` の `core_store.containers` の各容器にも `remote_children`(§19.13 と同じ形、いなければ項目なし)を入れる。`recursive=true` の副系の中身にも入る。
- **変換木(ツリー)**: 写しの Anchor の木は自分の系の Anchor の木の後に並べる。写しの Anchor・写しの始点の行は破線の印と「写し」の印を付け、名前は ID の名前の部分(`map`)、ツールチップに ID と取り込んだ仲介。写しの始点の行には、そこから出るまたぐ Transform の取り込みの状態(`ok` = 良好、`stale` = 途絶中、`lost` = 切断。複数なら悪い方)を出す。写しの行は容器ではないので選べない。
- **容器の行**: 別 System から来る Transform の取り込みが `ok` でない容器に状態の印(`lost` で写しが消えると変換木から外れるため)。`remote_children` がある容器に「別系の子」の印(ツールチップに系と終点)。副系の容器にも出す。
- **性質欄**: またぐ Transform の「親」に写しの名前と仲介、取り込みの状態・相手の系(表示中の系に UUID が見つかればその系へのリンク、なければ仲介と UUID)・最後に取れた時刻・周期・timeout。`remote_children` がある容器に「別系の子」の節(系・子の容器・最後に来た時刻。子が表示中の副系の容器ならリンク)。位置姿勢の基準の候補に写しの Anchor が入る(名前に「写し」と仲介)。
- **副系の容器の位置姿勢**: 副系の木は取り込まない(§19.3)ので変換木・変換は今までどおり最上位の系だけ。ただし副系がまたぐ Transform で最上位の系の容器 P を取り込んでいれば(P の `remote_children` にその副系の UUID がある)、P と P が入っている木の Anchor を基準の候補にして、最上位の系の `container/pose`(§19.8 の回し方)で副系の容器の位置姿勢を出す。候補がなければ出さない。
- 時刻は手元の時計の表記(年-月-日 時:分:秒.ミリ秒)。stamp は相手の System の時計の値のまま(§19.11)。
- 用語: 写し = Replica、別系の子 = Remote child。
