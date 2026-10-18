# juiz (Python)

[JUIZ](https://openjuiz.github.io/) のロボット・IoT 向けミドルウェアの `juiz` コマンドを、
Rust のツールチェーンなしで `pip` から入れるためのパッケージです。

```sh
pip install juiz
juiz --help
```

- `juiz` コマンドは Python のプロセスの中で動き、Python で書いた Process・Container・Component を読み込みます。
- Process などを書くためのデコレータ(`import juiz`)は [pyjuiz](https://pypi.org/project/pyjuiz/) にあり、このパッケージと一緒に入ります。
- `cargo install juiz_app` で `juiz` を入れた人は、このパッケージではなく `pip install pyjuiz` だけを追加してください。
  両方入れると `juiz` コマンドが2つになり、どちらが動くかが PATH の順番で決まってしまいます。
- この `juiz` で Rust のプラグインを使う場合は、プラグインをこの `juiz` と同じ版の juiz_sdk・コンパイラでビルドする必要があります。
  食い違うときは読み込み時にエラーで止まります。

## ビルド

[maturin](https://www.maturin.rs/) で wheel を作ります。

```sh
cd bindings/juiz_py
maturin build --release        # target/wheels/ に wheel を作る
maturin develop --release      # 今の仮想環境に入れる
```

## ライセンス

MIT OR Apache-2.0
