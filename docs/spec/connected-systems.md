# つながった System をたどる(再帰の一覧と system/profile)

issue #559。関係: #116(副系の表示)、#449(Topic の輪)、#478(マスターシステムの複数化)。

## 1. 目的

`juiz run` のアプリは基礎(例 `mobile_sim`)を自分の `subsystems` に入れてつなぐので、基礎から見るとアプリは
マスターシステムになる。以前の再帰はサブシステムしかたどらなかったため、基礎の `/ui` にはアプリが出なかった。

接続の向き(親子)は変えずに、つながっている System を全部たどる。どの System から問い合わせても、
同じ System の集まりが出る。

## 2. たどる相手

- マスターシステム(`mastersystem`)とサブシステム(`subsystems`)の両方。マスターシステムが先。
- 同じ System(UUID が同じ)が両方に居れば 1 度だけ(`add_subsystem` で互いに subsystem にした場合など)。
- 通ってきた System の UUID の列 `via` に入っている System と、自分には問い合わせない(輪で戻らない)。

`via` は param で送る。書き方は Topic の `via`(#449)と同じ、`,` 区切りの UUID。HTTP ではクエリ `via=`。

## 3. `system/profile?recursive=true`

- `subsystems` と `mastersystem`(配列)の各要素に、相手の System の `system/profile?recursive=true` の答えを入れ、
  経由する BrokerProxy の profile を `broker_proxy` に添える。
- 相手に渡す `via` は、それまでに中身を出した System を全部含む(深さ優先で、出した木の UUID を足していく)。
  なので System が何度つながっていても、中身は 1 度だけ出る。
- ほかの所で中身を出した System は、BrokerProxy の profile に `"visited": true` と `"system_uuid"` を添えて入れる。
- 取れなかった System は、BrokerProxy の profile に `"error"` と `"system_uuid"` を添えて入れる(以前と同じく続ける)。
- `recursive` を付けないときの `mastersystem`・`subsystems` は以前のまま(BrokerProxy の profile だけ)。

## 4. 再帰の一覧(`process/list?recursive=true` など)

`process`・`container`・`container_process`・`any_process`・`execution_context`・`broker`・`connection` の `list`。

- 相手に渡す `via` は、受けた `via` + 自分 + 自分がたどる相手全部。相手どうしがつながっていても、
  そちらからはたどらない。長い輪(4 つ以上)では同じものが 2 度出ることがある。
- 相手が答えないときは警告にして、その相手の分を空にして続ける(以前は一覧全体がエラーだった)。
- 識別子のブローカー部分は、以前と同じく直接つながった相手へのブローカーに書き換える。
- CLI の `System::process_list` なども、自分とつながった System を `via` に入れて問い合わせる。

実装では、一覧の BrokerProxy の引数を変えずに済むよう `via` をスレッドローカルで運ぶ(`juiz_core::brokers::walk`)。

## 5. 管理画面

- `mastersystem` のうち中身のあるもの(と `error` のもの)を「親系」(Master system)としてツリーと図の枠に出す。
  `visited` のものは出さない。
- 古い juiz の `mastersystem`(BrokerProxy の profile だけ)は出さない。
