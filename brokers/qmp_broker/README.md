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

画像などの大きなデータは、IPC ブローカーと同じ生バイトのフレーム(先頭が `JZF1`、[docs/spec/large-data.md](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/large-data.md) §3)で送ります。0.0.26 より前の qmp(MessagePack)とは互換がないので、つなぐ両方の juiz の版をそろえてください。
1 回の要求・応答の大きさの上限は、manifest の `max_frame_bytes`(既定 1 GiB)で変えられます。
