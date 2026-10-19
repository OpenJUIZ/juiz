# juiz_qmp_broker

[JUIZ](https://github.com/OpenJUIZ/juiz) の QUIC (qmp) ブローカーです。

`juiz` コマンド(クレート `juiz_app`)には既定で組み込まれているので、設定ファイルに次のように書くだけで使えます。

```yaml
brokers:
  - type_name: qmp
    name: qmp0
    port: 5001
```

自分で `System` を作るアプリケーションでは、`setup()` の前に `qmp_broker::register()` を呼んでください。
