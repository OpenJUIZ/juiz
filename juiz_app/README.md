# juizアプリケーションクレート


## juizコマンド

### 使い方

#### インストール
```
cargo install juiz_app
```

#### 単体オブジェクトのスロード
以下のように--process, --container, --container_process, --componentのオプションの引数で、コンテナやプロセスを単体でロードすることができる。
この時、-1オプションでロードしたモジュール一つにつき1つの実体化を行い、-eオプションでexecuteを行うことができる。executeはデフォルトの引数で実行することになるので、デフォルトの出力を確認することができるはずである。
-dオプションを使えばロードしてオプションが指定されていれば実体化し、実行したのちにもサーバーが停止せずに待機状態になる。
この状態でhttp://localhost:8000/docsに接続すれば、Swagger-UIで動作確認をすることができる。

以下にlibtalker.dylibファイルからプロセスをロードして実体化し、待機するコマンドを示す。
このdylibファイルはC++のプロジェクトから生成したプロセスであるので-lオプションでcppを指定した。
```
juiz --process target/debug/libtalker.dylib -l cpp -d -1 
```

以下に、単体ロード関連のjuizコマンドの引数を示す。
|オプション | 短いオプション | 説明 |
| --process | | プロセスを読み込みます。引数にプロセスの定義されているファイルを指定します。 |
| --container | | コンテナを読み込みます。引数にプロセスの定義されているファイルを指定します。 |
| --container_process | | コンテナプロセスを読み込みます。引数にプロセスの定義されているファイルを指定します。利用するコンテナを同時に読み込んでない場合は失敗します。 |
| --component | | コンポーネントを読み込みます。 |
| --language | -l | 読み込むモジュールの言語を指定します。 [cpp|python|rust]。デフォルトはrustです。 |
| --daemon | -d | ロードした後にサーバーとして待機状態になります。-dを指定しなければロード後に停止します。-eオプションを指定していれば実行後に停止します。 |
| -1 | | ロードしたモジュールからインスタンスを一つずつ作成します。コンポーネントの場合は、コンポーネントマニフェストに追加された順に実体化します。 |
| --execute | -e | 作成したインスタンスがプロセスないしはコンテナプロセスの場合は、それらを作成された順序で実行します。


#### yamlファイルを使ったプロジェクトの記述とロード

#### ネームスペースを指定して起動する

`--namespace` で設定ファイルのトップレベル `namespace`(System の既定ネームスペース)を置き換えて起動できる。
同じ設定ファイルを別のネームスペースで 2 つ起動すれば、同じ名前の Process を並べて動かせる。
2 つ目は 1 つ目と同じポートを指定しているので、1 つ目のサブシステムになり次のポートで待ち受ける。

```
juiz -f robot.conf --namespace robot1 -d
juiz -f robot.conf --namespace robot2 -d
```

`/` を指定するとルートになる。名前に `:` `|` を含むネームスペースや空の要素(`robot1//arm`)はエラーになる。

#### 一覧の絞り込みと表示

`process list` / `container list` / `container-process list` / `ec list` は `--namespace` で、
そのネームスペースとその下のオブジェクトだけに絞り込める。`--long` でネームスペース・名前・型の列を付けた表になる。

```
$ juiz process list --namespace robot2 --long
NAMESPACE  NAME  TYPE               ID
/robot2    inc0  increment_process  http://127.0.0.1:8001/process/robot2/inc0::increment_process
```

ネームスペース付きの ID はそのまま `process call` などに渡せる。

```
juiz process call http://127.0.0.1:8001/process/robot2/inc0::increment_process '{"arg1": 1}' -p
```
