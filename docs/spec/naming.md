# ネーミングサービス仕様

- 状態: 確定(ネーミングサービス Step 0、issue #615、2026-11-25)。§11・§12 は枠だけで、Step 4(#619)・Step 5(#620)で書き足す
- 親 issue: #289。計画と経緯はプロジェクトの計画書 naming-service-plan.md(2026-11-25 確定)にある
- 関連: [app-package.md](app-package.md) §10.2・§15.4〜§15.6(`juiz run` のつなぎ先)、[ipc-broker.md](ipc-broker.md) §2.2(置き場所と権限)、[connected-systems.md](connected-systems.md)(つながった後の全体像)
- この文書は Step 1〜6(#616〜#620、site#124)の実装の基準になる。実装で仕様を変えるときは、同じ PR でこの文書を直す
- 「しなければならない」「エラーにする」は必須の規則、「してよい」は任意を表す

## 1. 目的と範囲

名前(パッケージ名・System の名前)から、その System につながるブローカーの接続先(`ipc://…`・`http://…`・`qmp://…`・`zenoh://…`)を引く。

- つながった後の全体像(ロボットにいる System の一覧)は、すでに `system/walk` と再帰の一覧(connected-systems.md)で取れる。この仕様が足すのは、**最初の 1 本を名前で張ること** と、**1 つの System が持つ複数のブローカーから、どれでつなぐかを選ぶこと**。
- 登録する単位は System(プロセス)。Process・Container の名前は登録しない(つないだ後は今の ID と `list` で引ける)。
- 3 つの手段を重ねる。中央の名前サーバー(ROS 1 の master、CORBA の Naming Service の形)は作らない。

| 手段 | 届く範囲 | 既定 | Step |
| --- | --- | --- | --- |
| 機械ごとの登録ファイル(§3〜§6) | 同じ機械(同じユーザー) | 登録も引くのも有効 | 1〜3 |
| LAN の自動発見 mDNS(§11) | マルチキャストが届く LAN | 広告は既定オフ、引くのは feature があれば有効 | 4 |
| 知っている juiz に聞く peers(§12) | URL を書いた先 | 書いたときだけ | 5 |

- 人が書いたもの(`--depend`、覚えたつなぎ先、conf の `subsystems`・`mastersystem`、`-s` の URL)は常に名前の解決より勝つ。今の conf とコマンドはそのまま動く(§8)。
- 常駐プロセス(デーモン)は作らない。心拍も持たない。死活は引くときに確かめる(§6)。

## 2. 用語

| 用語 | 意味 |
| --- | --- |
| 登録名 | System を引くための名前(§4)。ネームスペースと合わせて `robot1/mobile_sim` の形で書く |
| 登録 | 機械ごとの登録ファイル 1 つ(§3)、または mDNS の広告 1 つ(§11) |
| 候補 | 名前を引いて見つかった登録のうち、生きていると確かめたもの(§6) |
| 解決 | 名前から候補を集め、1 つを選び、ブローカーの URL を 1 つ決めること(§7) |

## 3. 機械ごとの登録ファイル

### 3.1 置き場所

`<dir>/names/` の下に、登録 1 つにつき 1 ファイル。

- `<dir>` は Unix では ipc-broker.md §2.2 の IPC のソケットの置き場所と同じ(`JUIZ_IPC_DIR` → `$XDG_RUNTIME_DIR/juiz` → `/tmp/juiz-<uid>`)。権限の規則も同じ(`<dir>` が無ければ 0700 で作る、`/tmp/juiz-<uid>` の所有者・モードが違えばエラー)。
- Windows では `JUIZ_IPC_DIR`、無ければ `%LOCALAPPDATA%\juiz\run`(名前付きパイプにはディレクトリが無いため、ここで決める)。
- `names/` が無ければ、登録する側がモード 0700 で作る。登録ファイルはモード 0600 にする(Windows は利用者のプロファイルの下なので権限を足さない)。
- ファイルのパスは、ネームスペースをディレクトリにして `names/<ns の要素>/…/<名前>.json`。ルートのネームスペースなら `names/<名前>.json`。
  - 例: `robot1/mobile_sim` → `names/robot1/mobile_sim.json`、`mobile_sim` → `names/mobile_sim.json`
  - 途中のディレクトリも 0700 で作る。解除で空になったディレクトリは消してよい(消せなくてもエラーにしない)。

Docker で `JUIZ_IPC_DIR` を共有したコンテナ同士も、同じ機械として互いを引ける(IPC と同じ)。

### 3.2 中身

UTF-8 の JSON のオブジェクト 1 つ。

```json
{
  "format": 1,
  "name": "mobile_sim",
  "namespace": "robot1",
  "uuid": "1b2c…",
  "pid": 12345,
  "host": "robot1-pc",
  "juiz_version": "0.3.2",
  "started_at": "2026-11-25T15:04:05+09:00",
  "package": { "name": "mobile_sim", "version": "0.4.0", "kind": "base" },
  "brokers": [
    { "type_name": "ipc",  "id": "ipc://mobile_sim" },
    { "type_name": "http", "id": "http://0.0.0.0:8001", "port": 8001 },
    { "type_name": "zenoh", "id": "zenoh://0.0.0.0:7448", "port": 7448 }
  ]
}
```

| キー | 必須 | 中身 |
| --- | --- | --- |
| `format` | ○ | この形の版。今は `1`。読む側は知らない版(2 以上)の登録を、警告をログに出して飛ばす |
| `name` | ○ | 登録名の名前の部分(§4.1) |
| `namespace` | ○ | System の既定ネームスペース(namespace-plan の `--namespace`)。ルートは `""` |
| `uuid` | ○ | `system/profile` の `system_store.uuid`。死活の確かめ(§6)で、pid の使い回しと見分けるのに使う |
| `pid` | ○ | System のプロセス ID |
| `host` | ○ | 機械のホスト名(表示用。§11 で同じ機械かを見分けるのにも使う) |
| `juiz_version` | ○ | juiz の版(`juiz_core` の版) |
| `started_at` | ○ | 登録した時刻(RFC 3339) |
| `package` | | `juiz run` で起動したときの `package`(app-package.md §10.1 と同じ形)。それ以外は書かない |
| `brokers` | ○ | 立てたブローカーの一覧(§3.3)。空でもよい(引かれてもつなげないので、候補からは外れる) |

- 知らないキーは読み飛ばす(後の版で足せるように)。
- 書くときは `create_new`(既にあれば失敗)で開き、内容を書いて閉じる(§5.1)。読む側は、JSON として読めないファイルを「書いている途中」とみなして飛ばす。作られてから 10 秒より古くて読めないファイルは壊れたものとして消してよい。

### 3.3 ブローカーの一覧

`brokers` の各要素は、`System` が立て終わったブローカーの profile から作る。

| キー | 中身 |
| --- | --- |
| `type_name` | `ipc`・`http`・`qmp`・`zenoh` など(ブローカーの種類名) |
| `id` | ブローカーの ID(`<type_name>://<name>`)。待ち受けたアドレスをそのまま書く(`http://0.0.0.0:8001`) |
| `port` | ポートを持つ種類なら、実際に取れた番号(app-package.md §16 の取り直しの後の番号) |

- `id` の host が `0.0.0.0`・`::`・`[::]`・空なら、同じ機械から引くときは `127.0.0.1` に置き換えてつなぐ(§7.3)。別の機械に教えるとき(§11・§12)は、そのときに届いたアドレスに置き換える。
- 起動時に立てた後で足したブローカー(API でブローカーを作ったとき)は載せない(今は作れないので、作れるようになったら決める)。

## 4. 登録名

### 4.1 名前

| 起動のしかた | 名前 |
| --- | --- |
| `juiz run <パッケージ>` | パッケージ名(app-package.md §4.2) |
| `juiz -f <conf>`(`-d` 付きも) | conf の最上位の `name` |
| `juiz -d`(conf なし)など、`name` が無いとき | 登録しない |

- 名前の文字は ipc-broker.md §2.1 のブローカーの名前と同じ(`[A-Za-z0-9_-]` と `.` からなる 1〜64 文字、`.` で始まらない)。ファイル名にするため。
- conf の `name` がこの規則に合わないときは、`warning: '<name>' cannot be used as a registered name (…); this juiz is not registered in the name list` を標準エラーに出して登録しない(起動は止めない。今の conf を壊さないため)。
- conf の `option.naming.register: false`、または `--no-register` で登録しない(§10)。

### 4.2 ネームスペース

- System の既定ネームスペース(conf の `namespace` か `--namespace`)を `namespace` に書く。
- ネームスペースの各要素も §4.1 の文字の規則に合わなければならない(ディレクトリ名にするため)。合わなければ §4.1 と同じ警告を出して登録しない。
- 引くときの書き方:

| 書き方 | 意味 |
| --- | --- |
| `mobile_sim` | どのネームスペースの `mobile_sim` でも候補にする |
| `robot1/mobile_sim` | ネームスペース `robot1` の `mobile_sim` だけ |
| `/mobile_sim` | ルートのネームスペースの `mobile_sim` だけ |
| `mobile_sim@robot1-pc` | 機械 `robot1-pc` の `mobile_sim` だけ(§11・§12 で意味を持つ。同じ機械の登録だけを見るときは、`host` が一致するものだけ) |

- 最後の `/` の後ろが名前、前がネームスペース。`@` の後ろが機械名。`@` はネームスペースと名前の後に 1 つだけ書ける。

### 4.3 同じ機械で同じ登録名

同じ機械(同じ `<dir>`)に、同じネームスペースで同じ名前の登録は 1 つしか置けない。

- 生きている登録(§6)がすでにあれば:
  - `kind: base` の `juiz run`: IPC 名が同じなので、その前に ipc-broker.md §2.5 の `already in use`(app-package.md §15.2)で止まる。ここに来ることは無い。
  - `juiz -f` と `kind: app` の `juiz run`: 今の動作(同じ conf を 2 つ起動して、HTTP のポートの取り直し・親への参加で 1 つのロボットにする、app-package.md §16)を壊さないため、起動は止めない。名前に `-2`、`-3`、… を付けて空いているものを使い、`warning: mobile_sim is already registered by another juiz (pid 12345); this juiz is registered as mobile_sim-2` を標準エラーに出す。
  - `option.naming.unique: true`(§10)なら、`mobile_sim is already registered by another juiz (pid 12345)` のエラーで起動を止める。
- 生きていない登録(§6 で死んでいると分かったもの)は消して、その名前を使う。

## 5. 登録と解除の時機

### 5.1 登録

- `System::run`・`System::run_and_do`・`System::wait_for_signal` で動き続ける System だけが登録する。CLI のクライアントとして一度だけ動く `run_and_do_once`(`juiz process list` などが内部で作る System)は登録しない。
- 登録はブローカーを立て終わった後(`start_brokers` の後)に行う。ポートを取り直した後の実際の番号を書くため。
- 手順: `names/`(と途中のディレクトリ)を作る → `<名前>.json` を `create_new` で開く → 失敗(既にある)したら §6 で確かめ、死んでいれば消して 1 回だけやり直す。生きていれば §4.3。
- 登録に失敗しても(権限、ディスクが一杯など)、System の起動は止めない。`warning: cannot register mobile_sim in the name list: <理由>` を標準エラーに出す。`option.naming.unique: true` のときだけ起動を止める。
- 登録したら `registered as robot1/mobile_sim` をログ(`info`)に出す。標準エラーには出さない。
- 実装(#616): `System::run`・`System::run_and_do`(渡した関数を呼ぶ前)・`System::wait_for_signal` の始めで登録する(登録済みなら何もしない)。`setup` の中でブローカーを立て、subsystems・mastersystem につないだ後になる。`unique: true` で止めるときは、つないだ相手から外れ(#303)、ブローカーを止めてからエラーを返す。
- `started_at` は UTC(`2026-11-25T06:04:05Z` の形)で書く。

### 5.2 解除

- `System::stop` で、`leave_connected_systems`(#303)の後、ブローカーを止める前にファイルを消す。止まりかけの System が引かれないように、ブローカーより先に消す。
- 消す前にファイルを読み、`uuid` が自分のものでなければ消さない(自分の登録が死んだとみなされ、別の System が同じ名前で登録し直した場合)。
- 異常終了(kill -9、落ちた)で残ったファイルは、引く側が §6 で見つけて消す。

## 6. 死活の確かめ

引くとき(§7)・登録するとき(§5.1)に、登録が生きているかを次の順で確かめる。

1. `pid` のプロセスがいなければ、死んでいる。ファイルを消す。
   - Unix: `kill(pid, 0)` が `ESRCH`。Windows: `OpenProcess` が失敗し、`GetLastError` が `ERROR_INVALID_PARAMETER`、または開けたが `GetExitCodeProcess` が `STILL_ACTIVE` でない(終わったがハンドルが残っている)。
2. `brokers` のうち、引く側がつなげる種類(§7.3 の順)で最初のものに `system/profile` を問い合わせる(1 秒まで)。
   - 応答の `system_store.uuid` が登録の `uuid` と同じなら、生きている。
   - 応答したが `uuid` が違えば、pid が使い回されたか、ポートを別の juiz が取った。死んでいるとみなしてファイルを消す。
   - 応答が無ければ(つながらない、タイムアウト)、次の種類のブローカーで同じことをする。どれも応答しなければ、**候補から外すが、ファイルは消さない**(pid は生きているので、一時的に詰まっているだけかもしれない)。
3. つなげる種類のブローカーが 1 つも無ければ、候補から外す(ファイルは消さない)。
   - つなげるが、引く側が `system/profile` を問い合わせる手段を持たない種類(Step 1 では zenoh)だけを持つ登録は、1 の pid の確かめだけで生きているとみなす。問い合わせられるのは、juiz_core だけなら ipc(feature `ipc-broker`)と http、juiz コマンドはこれに qmp(feature `qmp-broker`)を足したもの。

- 心拍は持たない(#303 で「一時的な不通で消えるのを避ける」と決めたのと同じ考え)。確かめるのは引くときだけ。
- `juiz name list`(§9)は 2 を省いてよい(`--check` で確かめる)。1 の pid の確かめは必ずする。

## 7. 解決

### 7.1 候補を集める

名前を受け取って、次の手段の登録を集め、§6 で確かめて候補にする。

1. この機械の登録ファイル(§3)
2. LAN(§11、feature `naming-mdns` が有効なとき)と知っている juiz(§12、`naming.peers` を書いたとき)。この機械の登録で候補が見つかったときは、ここは見ない(同じ機械のものを優先する。`--lan`(§9)で両方を見る)

- 名前の照合は §4.2 の書き方による。`juiz run` の依存(§8)では、登録名ではなく `package.name` で照合する。

### 7.2 候補が複数のとき

黙って 1 つを選ばない。

- 対話できるとき(app-package.md §15.1 と同じ条件): 候補を並べて選ばせる。

```
2 juiz are registered as mobile_sim:
  1) robot1/mobile_sim  0.4.0  pid 12345  (this machine)
  2) robot2/mobile_sim  0.4.0  at robot2-pc
choose [1]:
```

- 対話できないとき: 候補を並べたエラーにする。`mobile_sim is ambiguous: robot1/mobile_sim (this machine), robot2/mobile_sim@robot2-pc. Write the namespace or the machine (ex., robot1/mobile_sim).`
- 候補が 0 なら `mobile_sim is not registered on this machine.`(LAN・peers を見たときは `… on this machine or the LAN.`)。
- 選んだものは覚えない(覚えるのは app-package.md §15.6 の `depends.yaml` だけ)。

### 7.3 ブローカーを選ぶ

候補の `brokers` から、つなぐ 1 つを次の順で選ぶ。引く側に BrokerProxy が無い種類(feature で入っていない)は飛ばす。

| 候補の場所 | 順 |
| --- | --- |
| 同じ機械(§3 の登録) | `ipc` → `zenoh` → `qmp` → `http` |
| 別の機械(§11・§12) | `zenoh` → `qmp` → `http`(`ipc` は使えない) |

- 種類を指定されたら(`--depend mobile_sim=zenoh`、`name://mobile_sim?type=http`、conf の `subsystems` の `type_name`、§8)、その種類だけを見る。その種類が無ければ `mobile_sim has no zenoh broker (it has: ipc, http)` のエラー。
- 選んだブローカーの URL は、`id` の host を §3.3 のとおり置き換えたもの(`http://127.0.0.1:8001`)。
- 同じ機械で ipc を先にするのは、速く、ポートを使わず、ファイルの権限で守られるため。別の機械で zenoh を先にするのは、zenoh-broker.md の計測で qmp・http より速いため。

## 8. `juiz run` のつなぎ先との両立

app-package.md §15.6 の「つなぎ先の順」を次にする(太字が足すもの)。

1. `--depend <名前>=<URL>`
2. 覚えた先(`depends.yaml`)
3. conf の `subsystems` のうち `package: <名前>` を持つもの
4. **この機械の登録(§3)で `package.name` が `<名前>`、`package.kind` が `base`、版が範囲に入るもの**
5. **LAN(§11)と知っている juiz(§12)の登録で、4 と同じ条件のもの**(Step 4・5 から)
6. `ipc://<名前>`(今の既定。登録を持たない古い juiz の基礎のため残す)

- 1〜3 は人が書いたものなので、常に名前の解決より勝つ。今の conf・`--depend` はそのまま動く。
- `--depend <名前>=<種類>`(`ipc`・`http`・`qmp`・`zenoh` のどれか、`://` を含まない)は、URL ではなく「4・5 で引くときの種類の指定」とする(§7.3)。今の `--depend <名前>=none`(#452)と同じく、URL でない値として見分ける。
- conf の `subsystems` に `package: <名前>` と `type_name` だけを書き、`name` を省いたときも、4・5 で引いて種類だけ決める。
- conf の `subsystems` の `type_name: http` で host だけを書き、ポートを省いたとき(app-package.md §10.2): host がこの機械(`localhost`・`127.0.0.1`・`::1`)なら、まずこの機械の登録の `port` を使い、無ければ今までどおり `base.http_port`。別の host なら、§11・§12 でその host の登録が見つかればその `port`、無ければ `base.http_port`。
- `kind: base` は同じ機械に 1 つしか動かない(IPC 名が同じため、app-package.md §10.1)ので、4 で候補が複数になることは無い。5 で複数になったら §7.2 による。
- 4・5 で決まったつなぎ先を使ったときは `using robot_base at http://127.0.0.1:8101 (found in the name list)` を標準エラーに出す(app-package.md §15.6 の覚えた先の文と同じ扱い)。
- subsystem につなぐと、相手は同じ種類でつなぎ返す(app-package.md §10.2)。今の `juiz run` は ipc と http のときだけアプリ側に受けのブローカーを足すので、zenoh・qmp を選んだときも足す(Step 2 #617 で入れる)。
- `package://<名前>/…`(app-package.md §10.4)は書き方を変えない。上の順で決まったつなぎ先に置き換わる。
- app-package.md §15.5(動いている基礎を探す)は、この機械の登録の一覧で行う。IPC の待ち受けの列挙は、登録を持たない古い juiz の基礎のために残す(同じ System が両方で見つかったら `uuid` で 1 つにまとめる)。§15.5 の「HTTP・qmp で動いている基礎は探さない」は、登録があれば探せるようになる。
- `base.http_port` は「登録が無いときの既定」に下がる。基礎のポートが逃げても(app-package.md §15.2 の `note:`)、登録があれば見つかる。

## 9. 名前で指す書き方とコマンド

### 9.1 `name://`

ブローカーの URL を受け取る所では、`name://<登録名>` で名前を書ける。

```
name://mobile_sim
name://mobile_sim?ns=robot1
name://mobile_sim@robot1-pc
name://mobile_sim?type=http
name://mobile_sim/process/inc0::increment
```

- 受け付ける所: CLI の `-s`、`--depend`、conf の `subsystems`・`mastersystem` の `id`、ブローカー付きの ID(上の最後の例)、pyjuiz のつなぎ先。
- `name://` の後ろの最初の `/` までが登録名(名前と `@<機械>`)。ID の残り(`/process/…`)と見分けるため、ネームスペースは `/` で書かず、クエリ `ns=<ネームスペース>` で書く(ルートは `ns=/`)。
- クエリ(`ns`・`type`)は、ブローカーの URL として書く所(`-s`・`--depend`・`subsystems`・`mastersystem`・pyjuiz)だけで使える。ブローカー付きの ID には書けない(書けばエラー)。ID でネームスペースや種類を決めたいときは、`subsystems` に名前でつないでおき、ID はそのブローカーの URL で書く。
- `type=<種類>` は §7.3 の種類の指定。ほかのクエリはエラー。
- 解決は `CoreWorker::broker_proxy` で行う(すべての BrokerProxy の入口)。種類 `name` の BrokerProxy を求められたら §7 で解決し、決まった種類・名前の BrokerProxy を返す。同じ System の中では、同じ名前の解決結果を使い回す(つながらなくなったら次に求められたときに引き直す)。
- 一覧や profile に出す ID は、解決した後の実際の URL(`ipc://mobile_sim/…`)にする。`name://` のまま出さない。

### 9.2 CLI の `-s`

- `-s` の値に `://` も `:` も無ければ、名前とみなして §4.2 の書き方で引く(`juiz -s mobile_sim process list`、`juiz -s robot1/mobile_sim process list`)。`robot1/mobile_sim` は `name://mobile_sim?ns=robot1` と同じ。
- `://` を含むもの、`localhost:8000` のように `:` を含むものは今までどおり。登録名は `:` を含められない(§4.1)ので、見分けがつく。
- 既定(`-s` を書かないとき)は今のまま `http://127.0.0.1:8000`。名前の解決には変えない。
- 名前を解決したときは、決まった URL をログ(`info`)に出す。`-v` で見える。

### 9.3 `juiz name`

| コマンド | 動作 |
| --- | --- |
| `juiz name list` | この機械の登録を並べる(登録名・package・版・pid・ブローカーの URL)。死んでいる登録(§6 の 1)は消してから並べる |
| `juiz name list --check` | 各登録に `system/profile` を問い合わせ(§6 の 2)、応答しないものに印を付ける |
| `juiz name list --lan` | LAN と peers の登録も並べる(Step 4・5) |
| `juiz name resolve <登録名>` | §7 で解決し、選んだブローカーの URL を 1 行で出す。候補が複数なら §7.2 |
| `juiz name resolve <登録名> --all` | 候補をすべて、ブローカーの一覧と一緒に出す |

- どれも `--json` で JSON を出す(`juiz name list --json` の各要素は §3.2 の形に、`alive`(`--check` のとき)と `machine`(`this` か機械名)を足したもの)。
- `juiz name` はブローカーを立てない(System を作らない)。

### 9.4 API・pyjuiz・管理画面(Step 3)

- API `naming/list`(GET): この System がいる機械の登録の一覧を返す(§9.3 の `--json` と同じ形)。`?lan=true` で LAN・peers も(Step 4・5)。peers(§12)はこの API を相手に聞く。
- API `naming/resolve?name=<登録名>`(GET): §7 の候補をすべて返す(選ばない。選ぶのは呼んだ側)。
- pyjuiz: つなぎ先に `name://…` か、`://` と `:` を含まない文字列を渡せる(§9.2 と同じ規則)。解決は pyjuiz が登録ファイルを直接読んで行う(juiz を通さない)。
- 管理画面: 「この機械で動いている System」を `naming/list` で並べ、選ぶとその System の管理画面を開く。

## 10. conf とコマンドの引数

```yaml
option:
  naming:
    register: true      # 既定 true。false なら登録しない(§4.1)
    unique: false       # 既定 false。true なら同じ登録名があるとき起動を止める(§4.3・§5.1)
    advertise: false    # 既定 false。LAN に広告する(§11、Step 4)
    peers: []           # 知っている juiz の URL(§12、Step 5)
```

| 引数 | 効き目 | 対象 |
| --- | --- | --- |
| `--no-register` | `register: false` | `juiz -f`、`juiz run` |
| `--advertise` | `advertise: true`(Step 4) | `juiz -f`、`juiz run` |
| `--peer <URL>` | `peers` に足す(Step 5、何度でも書ける) | `juiz -f`、`juiz run`、`juiz name` |

- 引数は conf より優先する。
- `option.naming` の知らないキーは警告をログに出して無視する(#401 の Process の知らないキーと同じ扱い)。

## 11. LAN の自動発見(mDNS)— Step 4(#619)で書き足す

決まっていること:

- DNS-SD のサービス種別は `_juiz._tcp`。TXT に登録の中身(§3.2 の `name`・`namespace`・`uuid`・`juiz_version`・`package`・ブローカーの一覧)を載せる。
- 広告は既定オフ(`option.naming.advertise: true` か `--advertise` で出す)。HTTP ブローカーには認証が無いので、LAN 全体に「ここにある」と出すかは利用者が選ぶ。
- 引く側(発見)は、feature があれば既定で動く。
- juiz_core の既定オフの feature `naming-mdns` にする(zenoh と同じく、依存とビルド時間のため CI の別ジョブで試す)。Rust の実装は `mdns-sd` を使う予定。
- 止めるときは goodbye(TTL 0)を送る。異常終了で残ったものは TTL 切れに任せ、引いた側でも §6 の 2 で確かめる。
- マルチキャストが通らない網(Docker の bridge、一部の Wi-Fi、サブネット越え)では効かない。その場合は §12。

Step 4 で決めること: TXT の項目名と長さの上限(1 項目 255 バイト)を越えるときの扱い、ブローカーの host の埋め方、インスタンス名の付け方、同じ機械の見分け方、引くときの待ち時間。

## 12. 知っている juiz に聞く(peers)— Step 5(#620)で書き足す

決まっていること:

- conf の `option.naming.peers` か `--peer <URL>` に、知っている juiz の URL を並べる。
- 解決のとき、各 peer の API `naming/list`(§9.4)を呼び、その機械の登録を候補に足す。peer 自身の `system/walk`(connected-systems.md §4.1)で見える System も候補に足してよい。
- 新しいサーバーは作らない。peer は普通の juiz。

Step 5 で決めること: 待ち時間、peer が返す登録の host の置き換え、`JUIZ_NAMING_PEERS` のような環境変数を持つか。

## 13. テスト(Step 1〜3)

- 登録ファイルの書き込み・読み込み・解除を、`JUIZ_IPC_DIR` を一時ディレクトリにして試す(テストごとに別のディレクトリ)。
- 死活: 存在しない pid の登録が消えること、pid は生きているが `uuid` が違う登録が消えること、応答しない登録が候補から外れるが消えないこと。
- §4.3 の重複: 2 つ目が `-2` で登録されること、`unique: true` で止まること、解除で他人の登録を消さないこと。
- §7.2 の複数候補のエラーの文、§7.3 のブローカーの選び方(種類の指定を含む)。
- §8 の順: `--depend`・覚えた先・conf が登録より勝つこと、基礎の HTTP のポートが逃げても登録で見つかること(Step 2)。
- `-s <名前>` で CLI がつながること(Step 3)。
- 置き場所の権限は ipc-broker.md §8 と同じ方法で試す。

## 14. 対象外

- 認証・暗号化(HTTP・IPC・zenoh の今の扱いのまま)。
- Process・Container 単位の登録。
- サブネット越え・インターネット越しの自動発見(§12 で URL を書けば届く)。
- ROS 2 の discovery、OpenRTM のネームサービスとの相互接続。
- 中央の名前サーバー。

## 15. Step との対応

| Step | issue | この文書の節 |
| --- | --- | --- |
| 0 仕様書 | #615 | 全体 |
| 1 機械ごとの登録、`juiz name` | #616 | §3〜§7、§9.3、§10(`register`・`unique`)、§13 |
| 2 `juiz run` が使う | #617 | §8 |
| 3 CLI・pyjuiz・管理画面で名前を指す | #618 | §9.1・§9.2・§9.4 |
| 4 LAN(mDNS) | #619 | §11 |
| 5 知っている juiz(peers) | #620 | §12 |
| 6 サイトのガイド | site#124 | — |
| 7 リリース | #621 | — |
