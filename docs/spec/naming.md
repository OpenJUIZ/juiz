# ネーミングサービス仕様

- 状態: 確定(ネーミングサービス Step 0、issue #615、2026-11-25)。§11 は Step 4(#619)、§12 は Step 5(#620)で書き足した
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
- 候補が 0 なら `mobile_sim is not registered on this machine.`(LAN を見たときは `… on this machine or the LAN.`、peers に聞いたときは §12.2)。
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
- subsystem につなぐと、相手は同じ種類でつなぎ返す(app-package.md §10.2)。今の `juiz run` は ipc と http のときだけアプリ側に受けのブローカーを足すので、zenoh・qmp を選んだときも足す(Step 2 #617 で入れた)。
- `package://<名前>/…`(app-package.md §10.4)は書き方を変えない。上の順で決まったつなぎ先に置き換わる。
- app-package.md §15.5(動いている基礎を探す)は、この機械の登録の一覧で行う。IPC の待ち受けの列挙は、登録を持たない古い juiz の基礎のために残す(同じ System が両方で見つかったら `uuid` で 1 つにまとめる)。§15.5 の「HTTP・qmp で動いている基礎は探さない」は、登録があれば探せるようになる。
- `base.http_port` は「登録が無いときの既定」に下がる。基礎のポートが逃げても(app-package.md §15.2 の `note:`)、登録があれば見つかる。

#617 の実装で決めた細部:

- 4 は依存ごとに、`package.name` が依存の名前、`package.kind` が `base`、`package.version` が範囲に入る登録を集め、§6 で生きているものを候補にする(`juiz_app` の `package::depends::RegistryNames`)。ブローカーは §7.3 の同じ機械の順で、`juiz` コマンドがつなげる種類(`juiz name` と同じ)から選ぶ。
  - 候補が無い、または候補につなげる種類のブローカーが無いときは 6(`ipc://<名前>`)に任せる(エラーにしない)。候補が 2 つ以上あれば `robot_base is registered more than once on this machine: … Choose one with --depend robot_base=<URL>.` のエラー(基礎は同じ機械に 1 つなので、ふつうは起きない)。
  - 見つかった結果は 1 回の `juiz run` の中で使い回す。見つからなかった結果は使い回さない(§15.4 の「もう一度試す」の間に起動した基礎を見つけるため)。
- 種類の指定(`--depend <名前>=<種類>`、conf の `subsystems` で `name` を省いたもの)で、その種類のブローカーを持つ登録が無いとき: `ipc` は `ipc://<名前>`、`http` は `127.0.0.1` と `base.http_port`(今の「ポートを省いた http」と同じ)、`qmp`・`zenoh` は `robot_base is not registered on this machine, so its qmp broker cannot be found. Write where it is (ex., --depend robot_base=qmp://192.168.0.10:<port>).` のエラー。登録はあるがその種類を持たないときは §7.3 の `has no … broker` のエラー。
  - 種類として受け付けるのは `ipc`・`http`・`qmp`・`zenoh` だけ。ほかの `://` を含まない値は今までどおり URL の書式の誤り。
  - conf の `subsystems` で `name` を省いたときは、`type_name` も省いてよい(種類も登録で決める)。`timeout_ms` などのほかのキーは残す。
- 覚えた先(§15.6)は `--depend` と同じ扱いなので、`depends.yaml` に種類だけを書いても同じく引ける。
- `found in the name list` の文は、決まったつなぎ先が 6 の既定(`ipc://<名前>`)と同じなら出さない(基礎は必ず ipc を持ち、同じ機械では ipc が先なので、ふつうの起動で毎回出るのを避ける)。種類の指定やポートの補いで登録を使ったとき、ipc をつなげない juiz で別の種類を選んだときに出る。対話できずつながらなかったときは、§10.3 のエラーの後に添える(覚えた先の文と同じ扱い)。
- zenoh・qmp のつなぎ返し: conf にその種類のブローカーが無ければ `{type_name: qmp, name: qmp0, port: 0}`(zenoh も同じ形)を足す。ポートは空いている番号を OS が選び、名前は立てた後に `127.0.0.1:<ポート>` に置き換わる。`--depend` で URL を書いて zenoh・qmp でつなぐときも足す。
- 基礎の HTTP のポートが逃げたときの app-package.md §15.2 の添え書きは、登録するとき(`option.naming.register` が true、`--no-register` 無し)は `note: apps on this machine find robot_base on port 8101 in the name list, but apps on other machines look for port 8100.`、登録しないときは今の文。
- app-package.md §15.5 の「動いている基礎」は、登録の一覧(4 と同じ条件、ブローカーは §7.3 の順で 1 つ)に、IPC の待ち受けの列挙で見つかったものを足す。列挙で見つかったものの `system_store.uuid` が登録と同じなら足さない。

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
| `juiz name list --lan` | LAN と peers の登録も並べる(Step 4・5、§11.5・§12.2) |
| `juiz name list --peer <URL>` | 書いた peers の登録も並べる(Step 5、§12.2) |
| `juiz name resolve <登録名> --lan` | この機械と LAN の両方から候補を集める(§11.5) |
| `juiz name resolve <登録名>` | §7 で解決し、選んだブローカーの URL を 1 行で出す。候補が複数なら §7.2 |
| `juiz name resolve <登録名> --all` | 候補をすべて、ブローカーの一覧と一緒に出す |

- どれも `--json` で JSON を出す(`juiz name list --json` の各要素は §3.2 の形に、`alive`(`--check` のとき)と `machine`(`this` か機械名)を足したもの)。
- `juiz name` はブローカーを立てない(System を作らない)。

### 9.4 API・pyjuiz・管理画面(Step 3)

- API `naming/list`(GET): この System がいる機械の登録の一覧を返す(§9.3 の `--json` と同じ形)。`?lan=true` で LAN・peers も(Step 4・5)。peers(§12)はこの API を `?connected=true` を付けて相手に聞く(§12.4)。
- API `naming/resolve?name=<登録名>`(GET): §7 の候補をすべて返す(選ばない。選ぶのは呼んだ側)。
- pyjuiz: つなぎ先に `name://…` か、`://` と `:` を含まない文字列を渡せる(§9.2 と同じ規則)。解決は pyjuiz が登録ファイルを直接読んで行う(juiz を通さない)。
- 管理画面: 「この機械で動いている System」を `naming/list` で並べ、選ぶとその System の管理画面を開く。

### 9.5 #618 の実装で決めた細部

- conf では、`subsystems` の要素に `type_name: name` と `name: <登録名>`(クエリも書ける。例 `name: "mobile_sim?type=http"`)、`mastersystem` に `broker_type: name` と `broker_name: <登録名>` を書く(どちらも今の書き方の種類を `name` にしたもの)。`juiz run` の `--depend <名前>=name://…` と conf の `subsystems` の `name://` は、`juiz run` が起動前に解決して実際の種類にする(つなぎ返しのブローカーを §8 のとおり足すため。決まったら `found in the name list` の文を出す)。
- 解決は `CoreWorker::broker_proxy`(と、conf の `subsystems`・`mastersystem`・CLI の `-s` が通る `SystemStorePtr::create_broker_proxy`)で行う。Process・Container・ContainerProcess・EC の Proxy を ID から作るときは、ID のブローカーの部分を解決した後の実際の URL に書き換えてから作る(一覧や profile に `name://` が出ない)。
- ID のブローカーの部分にクエリ(`?`)があればエラー `name://<…> can not have a query in an ID. …`。
- 同じ System の中の使い回し: 解決結果を `name://` の後ろの文字列ごとに覚え、次に使うときに相手の pid が生きていれば使う。死んでいれば引き直す(「つながらなくなったら」の判定を pid で行う。問い合わせを毎回しないため)。
- 死活の確かめ方とつなげる種類は、System の `set_naming_probe` で決めたもの(juiz コマンドは ipc・http・qmp と、feature があれば zenoh)。(#640 で変更)feature `zenoh-broker` は無くなり、juiz コマンドは ipc・http・qmp と、置き場所に入れた種類(`juiz broker install zenoh` など、broker-plugin-abi.md §10 の `used` の版がある種類)につなげる。
- CLI の `-s` は、サブコマンドがあるときだけ解決する。候補が複数で対話できるときは `juiz name resolve` と同じく選ばせる。解決したら `<-s の値> is resolved to <URL>` をログ(`info`)に出す。`name://` で書いた `-s` も CLI で解決する。
- API: `naming/list` は `?check=true` で §6 の 2 も確かめる(`juiz name list --check` と同じ)。`naming/resolve` の各要素は `naming/list` の要素と同じ形(`alive: true`)。どちらも IPC など CRUD のブローカーでも呼べる。
- pyjuiz: `juiz.proxy.connect(<名前>)`、`ProcessProxy(name, host=<登録名>, protocol='name')`(ContainerProxy も同じ)、`juiz.resolve_name`・`juiz.name_list`(モジュール `juiz.naming`)。つなげる種類は ipc → http の順。死んだ登録は飛ばすが、ファイルは消さない(消すのは juiz)。候補が 0 なら `NameNotRegisteredError`、複数なら `NameAmbiguousError`(文は §7.2 と同じ。対話はしない)。
- 管理画面: 左の欄の「ほかの系」で「このマシンで動いている系」を `naming/list?check=true` で並べる。HTTP のブローカーを持つ系は「開く」でその系の `/ui/` を別の画面に開く。待ち受けの host が `0.0.0.0`・`127.0.0.1` などなら、一覧を取った juiz の host に置き換える(別のマシンのブラウザからも届くように)。今見ている系には印を付ける。

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
| `--advertise` | `advertise: true`(Step 4、§11.2) | `juiz -f`、`juiz run` |
| `--peer <URL>` | `peers` に足す(Step 5、何度でも書ける) | `juiz -f`、`juiz run`、`juiz name` |

- 引数は conf より優先する。
- `option.naming` の知らないキーは警告をログに出して無視する(#401 の Process の知らないキーと同じ扱い)。

## 11. LAN の自動発見(mDNS)

Step 4(#619)で書き足した。

### 11.1 決まっていたこと

- DNS-SD のサービス種別は `_juiz._tcp`。TXT に登録の中身(§3.2 の `name`・`namespace`・`uuid`・`juiz_version`・`package`・ブローカーの一覧)を載せる。
- 広告は既定オフ(`option.naming.advertise: true` か `--advertise` で出す)。HTTP ブローカーには認証が無いので、LAN 全体に「ここにある」と出すかは利用者が選ぶ。
- 引く側(発見)は、feature があれば既定で動く。
- juiz_core の既定オフの feature `naming-mdns` にする(zenoh と同じく、依存とビルド時間のため CI の別ジョブで試す)。Rust の実装は `mdns-sd`(0.21 系、純 Rust、非同期ランタイムに依らない)を使う。`juiz_app` の同名の feature `naming-mdns` がこれを入れる。
- 止めるときは goodbye(TTL 0)を送る。異常終了で残ったものは TTL 切れに任せ、引いた側でも §6 の 2 で確かめる。
- マルチキャストが通らない網(Docker の bridge、一部の Wi-Fi、サブネット越え)では効かない。その場合は §12。

### 11.2 広告

- 広告するのは §5.1 で登録する System と同じ(動き続ける System)。時機も同じ(登録の直後、`System::stop` で登録を消す直前に goodbye)。
- `advertise` は `register` と別に効く。`register: false`(`--no-register`)でも `advertise: true` なら広告する(この機械の一覧には出ず、LAN だけに出る)。登録したときは、登録と同じ名前(§4.3 で `-2` を付けたもの)で出す。
- 広告したら `note: robot1/mobile_sim is advertised on the LAN (mDNS). Other machines on the LAN can find and call it.` を標準エラーに出す(認証の無いブローカーを LAN に出したことを利用者に見せる)。
- 広告できないときは警告を出して広告しない(起動は止めない)。
  - feature の無い juiz: `warning: robot1/mobile_sim is not advertised on the LAN: this juiz is built without the feature naming-mdns`
  - 他の機械からつなげるブローカーが無い(§11.3 で全部外れた)、mDNS が使えない: `warning: cannot advertise robot1/mobile_sim on the LAN: <理由>`
  - 名前が規則に合わない: §4.1 と同じ形の `warning: … this juiz is not advertised on the LAN`

### 11.3 載せるブローカーと host の埋め方

- `ipc` は載せない(別の機械からつなげない)。
- ブローカーの ID の host は、待ち受けが `0.0.0.0` でも `127.0.0.1` になることがある(http は常に `127.0.0.1:<port>`、zenoh は `0.0.0.0` を `127.0.0.1` にする)。そこで ID の host が `127.0.0.1`・`localhost`・`::1` のときは conf の `brokers` を見る:
  - 同じ種類の要素がすべて `host` に `127.0.0.1`・`localhost`・`::1` を書いていれば、この機械の中だけで待ち受けているので載せない。
  - そうでなければ(`host` を書いていない、`0.0.0.0` など。`-d` の HTTP ブローカーもこちら)、すべての住所で待ち受けているとみなし、host を `0.0.0.0` にして載せる。
- ID の host が具体的な住所(`192.168.0.10`)なら、そのまま載せる。
- 受け手は host が `0.0.0.0`・`::`・`[::]`・空のブローカーを、mDNS の応答の住所に置き換える。住所は IPv4(リンクローカルでないもの → リンクローカル)を先に、無ければ IPv6(リンクローカルでないもの、`[…]` で書く)。ループバックの住所は使わない。置き換えられないブローカーは捨てる。
- SRV のポートは、載せたブローカーの http → qmp → zenoh の順で最初のもの(DNS-SD の決まりで要るだけで、juiz は TXT のブローカーの一覧を使う)。

### 11.4 TXT の項目、インスタンス名、同じ機械の見分け方

| キー | 必須 | 中身 |
| --- | --- | --- |
| `f` | ○ | §3.2 の `format`(今は `1`)。読む側は知らない版を飛ばす |
| `name` | ○ | 登録名の名前の部分 |
| `ns` | ○ | ネームスペース(ルートは空) |
| `uuid` | ○ | `system_store.uuid` |
| `pid` | ○ | 表示用(別の機械の pid は確かめに使わない) |
| `host` | ○ | 機械のホスト名(`@<機械>` の照合と表示に使う) |
| `v` | ○ | juiz の版 |
| `t` | ○ | `started_at` |
| `pkg`・`pkgv`・`kind` | | `package.name`・`package.version`・`package.kind`(`juiz run` のときだけ) |
| `b0`、`b1`、… | | ブローカーの ID(§11.3 を通したもの、例 `http://0.0.0.0:8001`) |

- 1 項目(`<キー>=<値>`)は 255 バイトまで(RFC 6763 §6.1)。必須の項目が越えれば広告しない(長いネームスペースなど)。`pkg` などとブローカーは、越えた項目だけを載せず `warning: '<キー>=<値>' is longer than 255 bytes and is not advertised` を出す。
- インスタンス名は、登録名の `/` を `.` にして 54 バイトまでに切り、`-<uuid の先頭 8 文字>` を付けたもの(`robot1.mobile_sim-1b2c3d4e`)。同じ名前の juiz が LAN に何台いてもぶつからない。表示には使わない(表示は TXT から作る)。
- mDNS の A・AAAA レコードの名前は `juiz-<uuid の先頭 8 文字>.local.`。OS の応答者(Avahi・Bonjour)の機械名とぶつけないため。
- 同じ機械の見分け方: LAN で見つけた登録のうち、`uuid` がこの機械の登録ファイル(§3)のどれかと同じものは捨てる(同じ System をこの機械と LAN で 2 度出さない)。`host` の名前では見分けない(Docker で `JUIZ_IPC_DIR` を分けたコンテナ同士は、同じ機械でも LAN として見える)。

### 11.5 引く(発見)

- `_juiz._tcp` を 1.5 秒だけ探し、届いた応答の TXT から §3.2 の形の登録を作る。同じ `uuid` の応答が何度も届いたら(住所が揃うにつれて届き直す)、ブローカーが多く埋まったものを残す。
  - 1.5 秒にしたのは、mDNS の応答者が同じレコードを 1 秒に 1 回しか流さない(RFC 6762 §6)ため。直前に別の juiz が引いた直後でも、1 秒後の問い合わせの再送への応答で見つかる。
- 死活は §6 の 2・3 で確かめる。別の機械なので 1(pid)は見ない。ブローカーは §7.3 の別の機械の順(zenoh → qmp → http)で問い合わせ、応答の `uuid` が違えば捨てる(消すファイルは無い)。応答が無いもの・つなげる種類が無いものは候補にしない。問い合わせられない種類(zenoh)だけなら、見つかったことをもって生きているとみなす。
- 引く所と順(§7.1):
  - `juiz name resolve`、CLI の `-s <名前>`、`name://`(conf・ID・`juiz run` の `--depend`)、API `naming/resolve`: この機械の登録で候補が無かったときだけ LAN を見る。`juiz name resolve --lan`、`naming/resolve?lan=true` は両方を見る。
  - `juiz name list --lan`、API `naming/list?lan=true`: この機械の一覧の後に、LAN で見つけたものを並べる(既定は LAN を見ない。1.5 秒待つので)。
  - `juiz run` の依存(§8 の 5): 4(この機械の登録)で見つからなかったときだけ。条件は 4 と同じ(`package.name`・`kind: base`・版)。LAN で 2 つ以上見つかったら、対話できても選ばせず `robot_base is found more than once on the LAN: robot_base@robot2-pc, robot_base@robot3-pc. Choose one with --depend robot_base=<URL>.` のエラー(§7.2 の選ばせる形は Step 4 では入れない)。
- 候補が 0 で LAN も見たときは `… is not registered on this machine or the LAN.`(§7.2)。
- feature の無い juiz は LAN を見ない(今までどおり)。`--lan` を書けば `warning: --lan is ignored: this juiz is built without the feature naming-mdns` を出し、この機械の分だけを並べる。
- `juiz name list --lan` の表には `MACHINE` の列(この機械は `(this)`、LAN は機械名)を足す。LAN のものは ipc のブローカーを出さない。`--json` の要素は `machine` に機械名、`found: "lan"` を足す(この機械の要素は今までどおり `machine: "this"`)。
- 選んだブローカーの URL は、§11.3 で host を置き換えたもの(`http://192.168.0.12:8001`)。
- pyjuiz と管理画面は Step 4 では LAN を見ない(pyjuiz は登録ファイルを直接読むため。管理画面は `naming/list?lan=true` を使えば出せる)。

### 11.6 pip の juiz-cli の wheel

Step 7(#621)で決めた。

- 公開する juiz-cli の wheel には feature `naming-mdns` を入れる(juiz_py の同名の feature が `juiz_app/naming-mdns` を有効にする)。pip で入れた人も cargo で入れ直さずに LAN で名前を引け、広告もできる(§8.3 の Zenoh と同じ考え)。
- Zenoh(zenoh-broker.md §8.3)と違い、PR のビルドでも常に入れる(#640 からは Zenoh は wheel に入れない)。依存は純 Rust の `mdns-sd` だけで、Linux x86_64 の release ビルドで拡張モジュールが約 1.1 MB(45.1 → 46.2 MB、strip 前)増え、wheel は約 15 MB のまま。PR と公開で wheel の構成を変えずに済む。
- スモークテスト(`bindings/juiz_py/tests/smoke_test.py`)は `juiz name list --lan` が「feature が無い」警告を出さないことで、入っていることを確かめる。広告して見つける試験は CI の runner のマルチキャストに頼るので、wheel のジョブではしない(test-linux.yml の `naming-mdns` ジョブで確かめる)。

## 12. 知っている juiz に聞く(peers)

Step 5(#620)で書き足した。マルチキャストが通らない網(Docker の bridge、サブネット越え、一部の Wi-Fi)のため、知っている juiz の URL を書いておき、その juiz に名前の一覧を聞く。新しいサーバーは作らない。peer は普通の juiz。

### 12.1 書き方

- conf の `option.naming.peers`(URL の配列。1 つなら文字列でもよい)、引数 `--peer <URL>`(何度でも書ける。`juiz -f`・`juiz run`・`juiz name`)、環境変数 `JUIZ_NAMING_PEERS`(`,` か空白で区切る)。`--peer` は conf の `peers` に足す。3 つを合わせ、同じものは 1 つにする。
- URL は peer の HTTP ブローカー: `http://<host>[:<port>]`・`<host>[:<port>]`(`http://` を省ける)。ポートを省けば `8000`。IPv6 は `[fd00::1]:8000`。後ろのパスは無視する。
- peer に聞けるのは http だけ。ほかの種類(`qmp://…` など)は `peer 'qmp://…' is not an http URL. …` のエラー(`--peer`・`juiz name` の引数)か、警告をログに出して無視する(conf・環境変数)。
- 環境変数は、conf を持たない `juiz name` と CLI の `-s <名前>` でも使う。Docker のコンテナに 1 行足すだけで済むようにするため。

### 12.2 聞く

- 聞く所と順は LAN(§11.5)と同じ段(§7.1 の 2、§8 の 5)。この機械の登録で候補が無かったときだけ聞く(`--lan`・`naming/resolve?lan=true` は両方)。LAN と peers は並行に見る。
- 各 peer の API `naming/list?connected=true`(§9.4、§12.4)を HTTP の GET で呼ぶ。peer も並行に聞き、1 つ 1.5 秒まで待つ(LAN の待ちと同じ長さ。LAN と並行なので全体も 1.5 秒ほど)。
- 答えない peer(つながらない、タイムアウト、`naming/list` の無い古い juiz)は `warning: peer http://192.168.0.10:8000 did not answer: <理由>` を出し、ほかの所で探し続ける(エラーにしない)。
- 返った登録のうち、`uuid` がこの機械の登録と同じもの(peer が同じ機械にいて置き場所も同じとき)、LAN と peers で同じ `uuid` の 2 つ目は捨てる(LAN を先に残す)。
- 死活は LAN と同じ(§11.5): 別の機械なので pid は見ず、§6 の 2・3 を別の機械の順(zenoh → qmp → http)で確かめる。
- 一覧(`juiz name list`)は既定でこの機械だけ。`--lan` で LAN と peers(`JUIZ_NAMING_PEERS` を含む)、`--peer` を書けば書いた peers も並べる。API `naming/list?lan=true` は LAN とその System の conf・環境変数の peers。
- peer に聞いた候補の印は `found: "peer"`、`machine` は登録の `host`(peer の機械名)。表示は LAN と同じ(`robot2/mobile_sim  0.4.0  at robot2-pc`、`robot2/mobile_sim@robot2-pc`)。
- 候補が 0 で peers に聞いたときの文は `… is not registered on this machine or the peers.`、LAN も見たときは `… on this machine, the LAN or the peers.`。`juiz run` の依存で 2 つ以上見つかったときは `… is found more than once on the peers: …`(LAN と混ざれば `on the LAN and the peers`)。
- 聞くのは 1 段だけ。peer は自分の peers や LAN を見ない(`naming/list?connected=true` に `lan` を付けない)。輪になっても問い合わせが広がらない。

### 12.3 host の置き換え

- peer が返すブローカーの ID の host が、この機械の中だけを指すもの(`127.0.0.1`・`localhost`・`::1`)か、すべての住所(`0.0.0.0`・`::`・空)なら、peer の URL の host に置き換える(`http://127.0.0.1:8001` → `http://192.168.0.10:8001`)。peer の機械の登録の ID は、§11.3 の落とし穴のとおり待ち受けが `0.0.0.0` でも `127.0.0.1` になるため。
- 具体的な住所(`192.168.0.12`)はそのまま。`ipc` は捨てる(別の機械からつなげない)。
- 本当に `127.0.0.1` だけで待ち受けているブローカーは、置き換えた住所では応答しないので、§6 の 2 で候補から外れる(LAN のように conf の `brokers` は見ない。peer の conf はこちらから見えないため)。

### 12.4 peer がつないでいる System(`naming/list?connected=true`)

- `naming/list` に `connected=true` を付けると、その System の機械の登録の後に、その System がつないでいる System(`mastersystem`・`subsystems`、connected-systems.md)のうち**別の機械に http でつないだもの**の登録を足す。peers はこれを付けて聞くので、peer の機械の登録に加えて、peer がつないでいる別の機械の System も引ける。
  - 「別の機械」はつないでいる URL の host で見分ける(`127.0.0.1` などでない)。同じ機械の System(ipc・`127.0.0.1` でつないだもの)は、その機械の登録に出ているので足さない。
  - 足す登録は、つないでいる System に `naming/list` を聞き、`uuid` がつないでいる System と同じ要素を取ったもの。host は §12.3 と同じく、つないでいる URL の host に置き換える。答えない System は黙って飛ばす(ログの debug)。機械の登録と同じ `uuid` は足さない。
  - 足した要素には `found: "connected"` を付ける(`machine` は登録の `host`)。
- `system/walk`(connected-systems.md §4.1)は UUID の木しか返さないので使わない。つながった全体を引きたいときは、そこにつながった System を peer に書く。qmp・zenoh でつないだ System は足さない(peer の側で `naming/list` を聞く手段が http だけのため)。

### 12.5 使い回しと対象外

- `name://`(§9.1)を System の中で解決した結果の使い回しは、同じ機械の候補だけにする。別の機械(LAN・peers)の候補は pid を確かめられないので、毎回引き直す(Step 4 の LAN でも同じ扱いに直した)。
- pyjuiz と管理画面は peers を見ない(pyjuiz は登録ファイルを直接読むため。管理画面は `naming/list?lan=true` を使えば System の conf の peers の分も出る)。
- 認証は無い(§14)。peers に書いた juiz には、こちらから問い合わせるだけで、こちらの情報は送らない。

## 13. テスト(Step 1〜5)

- 登録ファイルの書き込み・読み込み・解除を、`JUIZ_IPC_DIR` を一時ディレクトリにして試す(テストごとに別のディレクトリ)。
- 死活: 存在しない pid の登録が消えること、pid は生きているが `uuid` が違う登録が消えること、応答しない登録が候補から外れるが消えないこと。
- §4.3 の重複: 2 つ目が `-2` で登録されること、`unique: true` で止まること、解除で他人の登録を消さないこと。
- §7.2 の複数候補のエラーの文、§7.3 のブローカーの選び方(種類の指定を含む)。
- §8 の順: `--depend`・覚えた先・conf が登録より勝つこと、基礎の HTTP のポートが逃げても登録で見つかること(Step 2)。
- `-s <名前>` で CLI がつながること、conf の `subsystems` に名前で書いた相手につながり一覧に実際の URL が出ること、API `naming/list`・`naming/resolve`(Step 3、`juiz_app` の `tests/cli/name_target.rs`、ポート 8160・8161)。pyjuiz は `test/unit/naming_test.py`、管理画面は `webui/tests/naming.test.ts`。
- 置き場所の権限は ipc-broker.md §8 と同じ方法で試す。
- Step 5: peer の URL の読み方・環境変数の分け方・`--peer` の書き足し・host の置き換え・`naming/list` の応答の読み方・答えない peer の警告は juiz_core の単体テスト(`naming::peers::tests`)。`juiz_app` の `tests/cli/peers.rs`(ポート 8180〜8182): 置き場所を分けた juiz コマンドから `--peer`・`JUIZ_NAMING_PEERS`・`-s` で peer の機械の登録を引けること、一覧の `found: "peer"`、答えない peer が警告になること、conf の `option.naming.peers` を書いた System が `subsystems` の名前と `naming/resolve` を peer で引くこと、peer が別の機械に http でつないだ System を `connected=true` で引けること(この機械のループバックでない IPv4 の住所を「別の機械」に見立てる。住所が無い環境ではその部分を飛ばす)。
- Step 4: TXT の読み書き・host の埋め方・載せるブローカー・インスタンス名・LAN の候補のブローカーの順は juiz_core の単体テスト(`naming::lan::tests`、feature なしで走る)。feature `naming-mdns` では同じプロセスで広告して見つけるテスト、`juiz_app` の `tests/cli/mdns.rs`(置き場所を分けた juiz コマンドから名前で引き、止めたら見つからない、ポート 8170)。`juiz run` の LAN の段は `package::depends` の単体テスト。CI は test-linux.yml の `naming-mdns` ジョブ(scripts/test-naming-mdns.sh)。マルチキャストが自分に戻ってくる網が要る。

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
