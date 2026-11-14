# juiz_zenoh_broker

[JUIZ](https://github.com/OpenJUIZ/juiz) の Zenoh ブローカーです。[Zenoh](https://zenoh.io/) の TCP で、別プロセス・別ホストの juiz を呼び出します。仕様は [docs/spec/zenoh-broker.md](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/zenoh-broker.md) です。

```yaml
brokers:
  - type_name: zenoh
    host: 0.0.0.0     # 省略すると 0.0.0.0
    port: 7448        # 省略すると 7448
```

繋ぐ側は `broker_proxies` / `subsystems` / `mastersystem` に `type_name: zenoh`、`name: <host>:<port>` を書くか、ID `zenoh://<host>:<port>` を使います。応答の待ち時間は `timeout_ms`(既定 10 秒)で変えられます。

- 要求と応答は、IPC・qmp と同じ生バイトのフレーム(先頭が `JZF1`、[docs/spec/large-data.md](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/large-data.md) §3)を、Zenoh の query / reply に 1 つずつ載せます。key expression は `juiz/v1/crud` に固定です。
- scouting(同じネットワークの Zenoh を自動で見つけること)は切ってあり、`tcp/<host>:<port>` の相手とだけ話します。
- 平文の TCP で、暗号化もアクセス制御もしません。信頼できるネットワークの中で使ってください。
- 1 回の要求・応答の大きさの上限は `max_frame_bytes`(既定 1 GiB)で、超えるフレームは送る前にエラーになります。
- Zenoh の設定は `zenoh_config`(`{"<キー>": <値>}`)で重ねられます。
- 同じホストの相手へは、64 KiB 以上のフレームを Zenoh の共有メモリで送ります(feature `shared-memory`、既定オン)。`shared_memory: false` で切れます。pool の大きさは `shm_pool_bytes`(既定 64 MiB)で、これより大きなフレームや、pool が空かないときは TCP で送ります。Zenoh は pool の全体を mlock するので、`ulimit -l`(RLIMIT_MEMLOCK、一般ユーザーの既定は多くの Linux で 8 MiB)が pool より小さいと警告を出して共有メモリを切ります。使うときは上限を上げるか `shm_pool_bytes` を下げてください。Zenoh の共有メモリの API は unstable なので、`default-features = false, features = ["plugin"]` にすると TCP だけでビルドできます。

`juiz` コマンドには feature `zenoh-broker` を付けてビルドすると組み込まれます。付けていない juiz では、設定ファイルの `plugins.broker_factories.zenoh_broker` でプラグイン(`libzenoh_broker.so` など)として読み込むか、自分で `System` を作るアプリケーションで `setup()` の前に `zenoh_broker::register()` を呼んでください。プラグインは Rust ABI で読み込むので、読み込む juiz と同じ rustc・同じ版でビルドしてください。

## テストとベンチ

Zenoh はビルドが重いので、このクレートはワークスペースの `default-members` に入っていません。ふだんの `cargo build` / `cargo test` ではビルドされません。

```sh
cargo build                                              # juiz コマンドとテストで使うプラグイン
cargo test -p juiz_zenoh_broker -- --test-threads=1      # このクレートのテスト (ポート 5212〜5254 を使う)
cargo test --release -p juiz_zenoh_broker --test zenoh_bench_test -- --ignored --nocapture --test-threads=1
```
