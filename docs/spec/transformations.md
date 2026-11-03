# Transformations(Container の位置姿勢と座標変換)仕様

- 状態: 確定(Step 0、issue #367、2026-11-02)
- 親 issue: #366。計画と経緯はプロジェクトの計画書 transformations-plan.md(2026-11-02 確定)にある
- この文書は Step 1〜7(#368〜#374)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
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
3. Transform の木と更新は 1 つの System の中で閉じる。システムをまたぐ Transform は対象外(§15、将来 #376)。
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
- 端点はどちらも同じ System の Container(Anchor を含む)でなければならない。見つからなければエラー。
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
- 別 System の Container(`ContainerProxy`)も口を持つが、登録所にはつながない(§15)。

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
- cjuiz(`juiz_sdk.h`)に、その口を取り出して読み書きするアクセサ関数を足す(仮の名前 `juiz_container_pose_get` / `juiz_container_pose_set`)。四元数の順番は (x, y, z, w)。
- 口を載せない古いホスト・口を知らない古いプラグインの組み合わせでも、今まで通り動かなければならない(口がなければアクセサはエラーコードを返す)。
- 関数名・引数・C の構造体・ABI の版の扱いは Step 5(#372)で決め、rust-plugin-abi.md と合わせて直す。

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

Anchor ごとの木と、各 Container の Pose の数値を表示する。3D 表示はしない(§15)。Step 6(#373)。

## 14. readonly ContainerProcess への備え

将来の readonly ContainerProcess(#377。Container を読み出しロックで並行に呼べる)に備え、読み出し口と書き込み口を API の上で分けておく:

- Rust は `pose()` を `&self`、`set_pose()` を `&mut self` に置く。
- C ABI・Python のアクセサも読み出しと書き込みを別の関数にする。
- readonly の ContainerProcess の中で書き込み口を呼ぶとエラーにする(この版では readonly がないので、規則だけ決めておく)。

## 15. 範囲の外

- システムをまたぐ Transform(サブシステムや別プロセスの Container との接続)。将来の Step 10(#376)。
- 過去の時刻の位置姿勢の問い合わせ(tf2 の時刻つきバッファ)。
- 管理画面の 3D 表示。
- readonly ContainerProcess の実装そのもの(#377)。
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
| `PoseNotWritableError` | 根の Anchor・基準自身・基準が子孫の Container への書き込み、readonly からの書き込み |
| `TransformAlreadyExistsError` | 同じ (source, destination) の Transform |
| `TransformTreeError` | §5.2 の木の規則を破る変更 |
| `TransformNotFoundError` | 指定の Transform がない |
| `PoseListenerError` | Pose 通知の登録の不備(引数・型・二重登録) |

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
