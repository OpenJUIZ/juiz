# Manifest と Profile の用語 仕様

- 状態: 確定(Step 0、issue #504、2026-11-15)
- 親 issue: #503。調査結果(main 6f5cfea 時点の食い違い 1〜6)は #503 の本文にある
- この文書は Step 1〜4(#505〜#508)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す。「(Step n で決める)」と書いた所は、その Step の PR でこの文書に書き足す

## 1. 目的と範囲

juiz のコード・API・文書では「manifest」と「profile」が混ざって使われてきた。宣言を Profile の型で持つ所、実行時の状態を持たないのに Profile と呼ぶ所、`_full` の無い `profile` の API が無いのに `profile_full` と呼ぶ所がある(#503 の 1〜6)。

この文書は 2 つの言葉の意味を決め、オブジェクトごとに「何が Manifest で、Profile では何が足されるか」を表にし(§4)、Step 1〜4 で変える型名・API 名と、古い名前を残す期間を決める(§6・§7)。

扱うのは Rust の型(juiz_sdk・juiz_core)、ブローカー API(HTTP・qmp・メッセンジャー系)、pyjuiz、CLI、管理画面の名前。JSON の形(キーと値)は変えない(§3 の 4)。

## 2. 用語

| 用語 | 意味 |
| --- | --- |
| **Manifest** | プラグイン・Process・Container などの**使われ方を決める宣言**。静的で、作った後に juiz が書き換えない。conf の各項目、プラグインの `manifest()`(マクロ・デコレータ・cppjuiz / hsjuiz の書き方が作るもの)、`.juizapp` のマニフェスト(`juizapp.yaml`)、API の `*/create` に渡す `__manifest__` がこれに当たる |
| **Factory の manifest** | プラグインが型ごとに出す Manifest。引数の既定値・型・出力の型・言語など、型で決まることを書く |
| **インスタンスの manifest** | Factory の manifest に、conf や `*/create` で渡した Manifest(名前・ネームスペース・引数の既定値の上書き・publish / subscribe など)を重ねた、完備な Manifest。インスタンスの実行と検査はこれを使う |
| **実行時の状態** | 動いている間に変わる値。拘束値・push で届いた値、EC の状態と束ねた Process、写しを取りに来ている System、Transform の今の値など(§4) |
| **Profile** | **使われている最中の様子**。インスタンスの manifest と実行時の状態を合わせたもの。Profile = Manifest + 実行時の状態 |

- 「profile」は API の `*/profile` の応答、`system/profile` の `core_store` の各要素、`*/create`・`*/destroy` が返すものを指す。
- 実行時の状態を持たないもの(Factory、Broker、PoseListener など、§4)の Profile は Manifest と同じ中身になる。それでも API では profile と呼ぶ(問い合わせの形をそろえるため)。

## 3. 規則

1. **Profile = Manifest + 実行時の状態。** Profile の型は Manifest の型を含む形にする(Rust では Manifest の型のフィールドを持ち、serde の `#[serde(flatten)]` で今の JSON の形を保つ)。Profile の型から Manifest を取り出せ、Manifest から状態を空にした Profile を作れなければならない。
2. **実行と検査は Manifest で行う。** 実行中のオブジェクトが自分のために持つ値(呼び出し前の検査 `check_manifest_before_call`、既定値の補い `fill_default_arguments`、Composite の子の検査)は Manifest の型でなければならない。Profile は問い合わせのたびに Manifest と状態から作る。
3. **宣言だけのものに Profile の名前を付けない。** 出力の型の宣言など、実行時の状態を含み得ないものは Manifest と呼ぶ(§6 の Step 2)。
4. **JSON の形は変えない。** 型名・関数名を変えても、API・conf・`system/profile` の JSON のキーと値は今のまま保たなければならない。他の版の juiz・pyjuiz・管理画面とつながるため。キーを足すのはよいが、消す・名前を変えるのは別の issue で決める。
5. **外から見える名前を変えるときは別名を残す**(§7)。

## 4. オブジェクトごとの Manifest と Profile

「Profile に足す実行時の状態」は main b0399eb(v0.2.10)の時点で Profile に載っているもの。「載せていないもの」は、状態ではあるが Profile に入っていないもので、足すかどうかはこの文書では決めない(足すなら別の issue)。

| オブジェクト | Manifest(Rust の型) | Manifest に入る主なもの | Profile に足す実行時の状態 | 載せていないもの | profile の API |
| --- | --- | --- | --- | --- | --- |
| Process | `ProcessManifest` | 名前・ネームスペース・型名・引数(型・既定値・schema)・出力の型・`use_memo`・言語・publish / subscribe・`composition`(Composite) | 引数ごとの `value`(拘束値か push で届いた値、#112。値型でないとき・まだ無いときは出さない) | 接続(`connection/list` で取る)、束ねている EC、出力の値 | `process/profile` |
| ContainerProcess | `ProcessManifest`(conf の書き方は `ContainerProcessManifest`) | Process と同じ + `container_name`・`container_type`・`readonly` | Process と同じ | Process と同じ | `container_process/profile` |
| Container | `ContainerManifest` | 名前・ネームスペース・型名・引数・`args`・親・持つ ContainerProcess の manifest・`model` の宣言 | `remote_children`(写しを取りに来ている System、transformations.md §19.10)、`model`(ファイルから読み直したモデル情報、container-model.md §7)、`processes` の各要素の Process の状態 | 中身(利用者のデータ)、位置姿勢(`container/pose` で取る) | `container/profile` |
| CompositeContainer | conf の `composite_containers` の 1 項目(Factory は `CompositeManifest`) | Container と同じ + スロット名と参照先の名前 | Container と同じ + `slots`(スロット → 解決した Container の ID) | — | `container/profile` |
| Anchor | `ContainerManifest` + `AnchorManifest` | Container と同じ + `anchor: {kind, rate}` | Container と同じ | 変換木での位置(`transformations/tree` で取る) | `container/profile` |
| Connection | `ConnectionManifest` | 種類(push / pull)・送り元・送り先・引数名 | なし(今は Manifest と同じ中身) | 最後に流れた時刻・値 | `connection/profile` |
| Topic | `TopicManifest`(名前だけ。conf の Topic の項目に共有メモリの設定 `shared_memory` を書く。Topic 自体は内部で Process として持つ) | 名前・ネームスペース・共有メモリの設定 | 共有メモリの数え上げ `shared_memory`(large-data.md §6.8) | 転送先の System の一覧 | `system/profile` の `core_store.topics` |
| Broker | `BrokerManifest` | 型名・名前・説明 | なし | 受け付けているアドレス・接続数 | `broker/profile` |
| EC | conf の `ecs` の 1 項目(Rust の型は無い、`Value`) | 型名・名前・ネームスペース・`rate`・`auto_start`・bind の宣言 | `state`(started / stopped)・`targets`(今束ねている Process の ID) | 周期の実測 | `execution_context/profile` |
| Factory(Process / Container / ContainerProcess / EC / Broker / Composite) | Factory の manifest(`ProcessManifest`・`ContainerManifest` など) | §2 の Factory の manifest + 読み込んだプラグインの `path`・`abi` | なし(§5) | 作ったインスタンスの一覧 | `system/profile` の `core_store.*_factories`、`composite/profile` |
| Transform | `TransformManifest` | 始点・終点・初期値・`writers`・`rate`・`timeout` | `transform`(今の値)・`stamp`、またぐ Transform の `remote: {system, rate, timeout, state, last_success}`(transformations.md §19.10) | — | `transform/profile` |
| PoseListener | `PoseListenerManifest` | 見張る Container・呼ぶ Process・引数・基準・`on_update`・`initial` | なし | 最後に呼んだ時刻 | `pose_listener/profile` |
| System | conf 全体(`.juizapp` から起動したときはその `package` も) | conf に書いたもの、`package: {name, version}`(app-package.md §10.1) | `system_store.uuid`・ブローカー、`core_store`(上の各 Profile の一覧)、`subsystems`・`mastersystem`(今つながっている System の profile) | — | `system/profile` |

- Topic の Rust の型: `TopicProfile` は `TopicManifest` を含むだけの型にした(状態なし、§8.1)。Topic の profile の API は Process の profile(`ProcessProfile`)で返り、`TopicProfile` は使わない。
- Broker の profile に今出ている `BrokerProfile` は Manifest と同じ中身。Step 1 で `BrokerManifest` を `#[serde(flatten)]` で含む形にした(§8.1)。

## 5. Factory の profile

**Factory は実行時の状態を持たないので、Factory の profile は Factory の manifest をそのまま返す**(#503 の 5)。

- 「そのまま」は意味の上の決めごと。JSON の形は今のまま(識別子などオブジェクトに共通の項目 + `plugin: {path, abi}` + 型の manifest)にする(§3 の 4)。プラグインの `path`・`abi` は conf の `plugins` の宣言から決まる静的な値なので、Manifest の一部と見なす。
- 作ったインスタンスの一覧などを Factory の profile に足してはならない。足したくなったら、それは Factory の状態ではなくインスタンスの一覧の問い合わせとして別の API にする。
- `composite/profile` は今も Factory の manifest(`ProcessManifest`)を返している。この決めごとに合っている。composition.md §8.4 の表の書き方もこの節を指すように直した。
- container-model.md §7 の「Factory の `profile_full` の `container_factory.model` に manifest に書いたままの値を載せる」もこの決めごとに合っている。

## 6. 名前の変更(Step 1〜4)

| Step | issue | 今の名前 | 新しい名前 | 外から見えるか | 別名 |
| --- | --- | --- | --- | --- | --- |
| 1 | #505 | `ProcessImpl` が持つ `profile: ProcessProfile`(process_impl.rs) | `manifest: ProcessManifest`。`profile()` は manifest + 状態から作る | いいえ | 不要 |
| 1 | #505 | `check_manifest_before_call(&ProcessProfile, …)`・`fill_default_arguments(&ProcessProfile, …)`(juiz_sdk の manifest_checker.rs) | 引数を `&ProcessManifest` に | Rust の SDK の公開関数 | なし(使っているのが core だけだったため。§8.1) |
| 1 | #505 | juiz_core の `argument_schema`・`check_argument_value`・`check_call_arguments`(types/mod.rs)、`process_openapi_spec`(types/openapi.rs)の `&ProcessProfile` | `&ProcessManifest`(profile しか無い所は `&profile` を渡すと Deref で manifest になる) | juiz_core の公開関数 | 不要 |
| 1 | #505 | `ContainerImpl` が持つ `profile: ContainerProfile`(juiz_sdk の container_impl.rs)と `profile_mut()` | `manifest: ContainerManifest` + profile に載せるスロット・Anchor の値。`profile_mut()` は `set_profile_slots()`・`set_profile_anchor()` に | Rust の SDK の公開関数(`profile_mut`) | なし(使っているのが core だけだったため。§8.1) |
| 1 | #505 | Composite の子の検査用の `ProcessProfile`(composite_process_factory.rs) | `ProcessManifest` | いいえ | 不要 |
| 1 | #505 | `ProcessProfile` / `ContainerProfile` / `ArgumentProfile` / `BrokerProfile` / `TopicProfile` の中身 | Manifest の型 + 状態のフィールド(§8.1 の表) | 型名は変えない。フィールドは変わる(`profile.name` は `profile.instance_name()` に) | 不要 |
| 1 | #505 | `ProcessProfile::build_instance_manifest` / `ContainerProfile::build_instance_manifest`(テストだけが使う) | 消す(`ProcessManifest::build_instance_manifest` 等を使う) | Rust の SDK の公開関数 | なし(使う所がテストだけのため、§7 の例外) |
| 2 | #506 | `OutputProfile` / `PrimitiveProfile` / `StructProfile`(juiz_sdk の output_profile.rs) | `OutputManifest` / `PrimitiveManifest` / `StructManifest`(ファイルも output_manifest.rs に) | Rust の SDK の公開型 | 古い名前を `#[deprecated] pub type` で残す |
| 2 | #506 | pyjuiz の `_returns_profile`、`PrimitiveProfile` 等(decorators.py・juiz.py) | `_returns_manifest`、`PrimitiveManifest` 等 | pyjuiz の公開名(`_` で始まるものを除く) | 古い名前を残す |
| 3 | #507 | `try_connect_to` が返す `ConnectionManifest`(`notify_connected_from` は `ConnectionProfile`)(juiz_sdk の process.rs) | どちらも `ConnectionProfile` | Rust の SDK のトレイト。ブローカー API `process/try_connect_to` の応答 | 不要(2 つの型は同じキーで直列化されるので応答の JSON は変わらない、§3 の 4) |
| 4 | #508 | API の `*/profile_full`(HTTP・qmp・メッセンジャー系。process・container・container_process・any_process・connection・execution_context・broker・system・composite・transform・pose_listener) | `*/profile`。`*/profile_full` は別名として残す | ブローカー API | 残す(§7) |
| 4 | #508 | `BrokerProxy` の `*_profile_full` 関数、pyjuiz の `profile_full()`、CLI・管理画面の呼び出し | `*_profile` / `profile()` | Rust の SDK・pyjuiz | Rust は `#[deprecated]` の関数、pyjuiz は古い名前を残す |

- 型・関数の中で Manifest と Profile の取り違えを見つけたら、その Step の範囲で直してよい。表に足して直す。
- C ABI(cjuiz・cppjuiz・hsjuiz)には Profile を名前に持つ型・関数は無いので、変えない。

## 7. 別名を残す期間

1. 外から見える名前(Rust の SDK の公開型・関数、pyjuiz の公開名、ブローカー API)を変えるときは、**古い名前を別名として残す**。Rust は `#[deprecated(note = "use …")]`、pyjuiz は古い名前から新しい名前を呼び、初めて使われたときに `DeprecationWarning` を出す。
2. **別名は 0.2 系のあいだ残す。** 消すのは 0.3.0 以降で、消すときは別の issue を立てる。release/0.1(不具合修正だけの枝)には名前の変更を持ち込まない。
3. **API の `*/profile_full` は 0.3.0 以降も、消す issue で決めるまで残す。** 古い版の juiz とつながる System(subsystems・mastersystem)や古い pyjuiz・管理画面が使うため。応答は `*/profile` と同じにし、OpenAPI では `deprecated: true` を付ける。
4. **新しい側から古い juiz を呼ぶとき**: CLI・pyjuiz・管理画面・`BrokerProxy` は `*/profile` を呼び、相手が 404(古い juiz で `*/profile` が無い)なら `*/profile_full` を呼び直さなければならない。相手の版を覚えるかどうか(毎回呼び直すか)は Step 4 で決める(§8.4 で、接続先ごとに覚えることにした)。
5. 公開の後に古い名前を使っている所(サイトのチュートリアル・ガイド、サンプル 4 言語)を新しい名前に直す。直す時期はその Step の公開の後で、Step ごとに決める。

## 8. Step ごとに決めたこと

(Step 2〜4 の PR で書き足す)

### 8.1 Step 1(#505)

**Profile の型の形。** どれも Manifest の型をフィールド `manifest` に持ち、`Deref` / `DerefMut` で Manifest の項目をそのまま読み書きできる(`profile.type_name`・`profile.arguments` など)。Manifest への変換は `into_manifest()` か `From`、Manifest から状態を空にした Profile は `TryFrom`(Process・Container は名前が無ければエラー)か `From`。

| 型 | Manifest | 状態のフィールド | JSON の作り方 |
| --- | --- | --- | --- |
| `ArgumentProfile` | `manifest: ArgumentManifest` | `value: Option<Value>` | `#[serde(flatten)]` |
| `ProcessProfile` | `manifest: ProcessManifest` | `argument_values: BTreeMap<String, Value>`(引数名 → 今の値) | 手書き(下の 1) |
| `ContainerProfile` | `manifest: ContainerManifest` | `processes: Vec<ProcessProfile>`・`slots`・`anchor`・`remote_children`・`model` | 手書き(下の 2) |
| `BrokerProfile` | `manifest: BrokerManifest` | なし | `#[serde(flatten)]` |
| `TopicProfile` | `manifest: TopicManifest` | なし | `#[serde(flatten)]` |

1. `ProcessProfile` の状態(引数の今の値)は JSON では `arguments` の各要素の `value` に入るので、`#[serde(flatten)]` だけでは書けない。Manifest を JSON にしてから各引数に `value` を差し込む直列化を手で書いた。読むときは `value` を抜き出して `argument_values` に入れ、残りを Manifest として読む(`value: null` は値なしとして扱う。今までの `Option<Value>` と同じ)。
2. `ContainerProfile` の `processes` と `model` は、Manifest にも同じ名前の項目(conf に書いた ContainerProcess の宣言、3D モデルの宣言)がある。JSON には状態の方(今ある ContainerProcess の profile、読み直したモデル情報)を出す。読むときは状態の項目を抜き出し、Manifest の `processes` には読んだ ContainerProcess の manifest を入れる。Rust では構造体のフィールドが `Deref` より先に見えるので、`profile.processes`・`profile.model` は状態、Manifest の宣言は `profile.manifest.processes` で読む。
3. **Manifest にあって今の profile の JSON に無い項目は出さない**(§3 の 4)。Process の `trace`、Container の `broker_type_name`・`broker_name`・`rate`。出すかどうかは別の issue で決める。
4. JSON が変わらないことは、HTTP は juiz_core の `core::profile_json`、qmp は juiz_qmp_broker の `profile_json` で確かめる。どちらも Step 1 の前(main d758bd9)の応答を写した golden の JSON と、キーと値が全部同じであることを見る。

**実行中のオブジェクトが持つ値。** `ProcessImpl` はインスタンスの `ProcessManifest` を持ち、`profile()` で問い合わせのたびに manifest と Inlet の今の値から `ProcessProfile` を作る。`ContainerImpl` も `ContainerManifest` を持ち、`profile()` で作る(CompositeContainer のスロットと Anchor の `{kind, rate}` は `set_profile_slots`・`set_profile_anchor` で持たせる)。呼び出し前の検査・既定値の補い・Composite の子の検査・型検査・`process/openapi_spec` は Manifest を受ける。

**別名。** `check_manifest_before_call`・`fill_default_arguments` の引数の型と `ContainerImpl::profile_mut` は SDK の公開名だが、使っているのが juiz_core だけ(サンプル 4 言語・pyjuiz・バインディングでは使っていない)なので別名を残さない。`ProcessProfile::build_instance_manifest`・`ContainerProfile::build_instance_manifest` は §6 のとおり消した(`ProcessManifest::build_instance_manifest` 等を使う)。

### 8.2 Step 2(#506)

**Rust の SDK。** `juiz_sdk::manifests` の `OutputProfile` / `PrimitiveProfile` / `StructProfile` を `OutputManifest` / `PrimitiveManifest` / `StructManifest` に変え、ファイルも `output_manifest.rs` にした。`ProcessManifest::outputs`・`set_outputs`・`ProcessProfile::outputs`・`set_outputs` も新しい型を使う。古い名前は `#[deprecated(since = "0.2.11")] pub type` で残し、`juiz_sdk::manifests` から今までどおり `use` できる(enum の別名なので `OutputProfile::Primitive(PrimitiveProfile { … })` もそのまま書ける)。

**pyjuiz。** `juiz.PrimitiveProfile` / `StructProfile` / `OutputProfile` を `PrimitiveManifest` / `StructManifest` / `OutputManifest` に変えた。古い名前はモジュールの `__getattr__` で新しいクラスを返し、名前ごとに初めて使われたときに `DeprecationWarning` を出す(`juiz.PrimitiveProfile`・`juiz.juiz.PrimitiveProfile`・`from juiz import PrimitiveProfile` のどれでも使える。`from juiz import *` には入らない)。`_returns_profile` は `_` で始まるので別名なしで `_returns_manifest` にした。

**変わらないもの。** JSON(`outputs` の `{"enum_type": …, "type_name": …, "default_value": …}`)は変わらない。serde のタグ `enum_type` の値は `Struct` / `Primitive` のまま。

**変わるもの。** `OutputManifest` の `Display` は `OutputManifest(int=0)` のように新しい名前を出す(今までは `OutputProfile(int=0)`)。ログとテストのための表示で、API の応答には使っていない。

**公開の後に直す所(§7 の 5)。** サンプル 4 言語・公式サイトで古い名前を使っている所は無い(2026-11-15 に確かめた)ので、直す所は無い。

### 8.3 Step 3(#507)

**Rust の SDK。** `Process::try_connect_to` の戻り値を `ConnectionManifest` から `ConnectionProfile` に変え、`notify_connected_from` とそろえた。どちらも、できた接続の `ConnectionProfile` を返す。`BrokerProxy`(`ProcessBrokerProxy`)の `process_try_connect_to` も同じく `ConnectionProfile` を返す。引数は今までどおり `&ConnectionManifest`(これから作る接続の宣言)。

**直した実装。** `Process` を実装しているのは `ProcessImpl`(ContainerProcess・Composite もこれを使う)・`ProcessProxy`・juiz_sdk のテスト用の `DummyProcess` だけ。`ProcessBrokerProxy` は `CoreBroker`・`CRUDBrokerProxyHolder`(HTTP・qmp・IPC・Zenoh)・`MessengerBrokerProxy`。

**別名は置かない。** トレイトの関数の戻り値なので別名にできない。名前は変わらず、外から呼ぶ側は `ConnectionProfile` を受け取るように直すだけでよい。juiz の外(サンプル 4 言語・teleop_twist・公式サイト)に `try_connect_to` を呼ぶ所・`Process` を実装する所は無い(2026-11-15 に確かめた)。

**変わらないもの。** ブローカー API `process/try_connect_to` の応答の JSON。2 つの型は serde で同じキー(`connection_type`・`source_process_id`・`destination_process_id`・`arg_name`)に直列化されるので、古い juiz の応答を新しい側が、新しい juiz の応答を古い側がそのまま読める(juiz_sdk の `connection_manifest_and_profile_serialize_alike` で確かめる)。`connection/create`・`connection/destroy`・`connection/profile_full` は前から `ConnectionProfile` を返していて、変えていない。

**公開の後に直す所(§7 の 5)。** 無い。

### 8.4 Step 4(#508)

**API。** 11 種類(process・container・container_process・any_process・connection・execution_context・broker・system・composite・transform・pose_listener)の READ の関数を `profile` にし、`profile_full` は同じ関数を指す別名にした。別名は CRUDBroker の READ の表を作るとき(`add_profile_full_aliases`)に足すので、HTTP・qmp・IPC・Zenoh・メッセンジャー系のどれでも同じに受ける。応答の JSON は変わらない(golden の core::profile_json・qmp の profile_json が `*/profile` で通る)。

**OpenAPI。** 定義は `/api/<class>/profile` に書き、`/api-docs/openapi.json` を返すときに `/api/<class>/profile_full` へ写して `deprecated: true` を付ける(`add_profile_full_paths`)。応答の型などは写す前に足すので、2 つは同じ。

**無い関数は 404。** HTTP ブローカーは、無い関数を呼ばれたとき(`CRUDBrokerCanNotFindFunctionError`)に 500 ではなく 404 を返すようにした(container-model.md §11 の振り分けに足した)。

**古い juiz を呼ぶとき(§7 の 4)。** 相手に `profile` が無いことの見分け方は、404、または古い juiz が返す 500 とメッセンジャー系の `BrokerProxyRemoteError` に入る「`CRUDBroker can not detect function (`」の文(`is_function_not_found`)。

- **相手の版は接続先ごとに覚える。** 一度 `profile_full` に呼び直したら、その接続先には以後はじめから `profile_full` を呼ぶ。Rust は BrokerProxy(`CRUDBrokerProxyHolder`・`MessengerBrokerProxy`)ごと、pyjuiz は `CRUDProxy`(`HttpProxy`・`IpcProxy`)ごと、管理画面は接続先の URL ごと。相手が新しい juiz に入れ替わっても `profile_full` は使えるので、覚えたままでよい。
- 呼び直すのは `profile` だけ。ほかのエラー(無い Process など)はそのまま返す。

**Rust の SDK。** `BrokerProxy` のトレイトの関数を `system_profile`・`system_profile_recursive`・`process_profile`・`container_profile`・`container_process_profile`・`any_process_profile`・`ec_profile`・`broker_profile`・`connection_profile`・`composite_profile`・`transform_profile`・`transform_profile_from_system`・`pose_listener_profile` にした。古い名前は `#[deprecated(since = "0.2.11")]` の既定の実装として残し、新しい名前を呼ぶ。実装する側(`CoreBroker`・`CRUDBrokerProxyHolder`・`MessengerBrokerProxy`)は新しい名前だけを実装する。

**変えなかったもの。** `JuizObject::profile_full()`(オブジェクトが自分の profile を JSON にする関数)と CoreStore の `*_profile_full`(`objects_profile_full` など)は juiz_core の中の関数で、API の名前ではないので、この Step では変えない。

**pyjuiz。** `SystemProxy.profile()` と、Process・Container・ContainerProcess の proxy の `profile(update=False)` を足した。`profile_full()` は名前ごとに初めの 1 回だけ `DeprecationWarning` を出して `profile()` を呼ぶ。

**管理画面。** `JuizClient` の `systemProfile`・`connectionProfile`・`transformProfile` が `*/profile` を呼び、古い juiz なら `*/profile_full` を呼び直す。`src/api/openapi.json` は新しい定義から取り直した。

**C ABI・CLI。** C ABI(cjuiz・cppjuiz・hsjuiz)に `profile_full` を呼ぶ所は無い。CLI は BrokerProxy を通すので、上の呼び直しがそのまま効く。

**公開の後に直す所(§7 の 5)。** 次の版の公開の後に `*/profile` に直す(2026-11-16 に確かめた)。

- サンプル 4 言語(C++・Haskell・Python・Rust): README の `curl .../api/container_process/profile_full` と tests/test_examples.py の `PROFILE` の読み出し。
- 公式サイト: ガイドの composition・container-model・large-data・transformations で `*/profile_full` を書いている所(16 か所)。
- teleop_twist: 無い。
