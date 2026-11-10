# juiz_pkg

[juiz](https://github.com/OpenJUIZ/juiz) のパッケージの雛形を作る道具です。Process・Container・Component を作り始めるときの「最初のプロジェクト」を、コマンド 1 つで作ります。作った雛形は、手を加えずにビルド → `juiz -f app/app.conf` で起動 → 呼び出し → `juiz pack` で `.juizapp` に、まで通ります。

```sh
juiz pkg create my_robot -b rust          # Rust の Component (Container + ContainerProcess + Process)
juiz pkg create my_robot -b python -t process
juiz pkg create my_robot -b cpp            # C++ (CMake、libjuiz_sdk を ~/.juiz/sdk/<版> に用意しておく)
juiz pkg create my_robot -b haskell        # Haskell (cabal、libjuiz_sdk と juiz のソースの hsjuiz を使う。Linux・macOS)
juiz pkg target                           # app/plugins/<target>/ の <target> (ホストの target triple)
```

- 言語 `-b`/`--lang`: `rust`、`python`、`cpp`、`haskell`
- 中身 `-t`/`--template`: `component`(既定)、`process`、`container`
- 雛形に書く juiz_sdk / pyjuiz / libjuiz_sdk / hsjuiz の版は、雛形を作った juiz 自身の版になります

仕様は [docs/spec/pkg.md](https://github.com/OpenJUIZ/juiz/blob/main/docs/spec/pkg.md) にあります。

## コマンド

`juiz` コマンド(`cargo install juiz_app`、`pip install juiz-cli`)に `juiz pkg` として入っています。これを使うと、雛形の版が juiz と必ずそろいます。

このクレートだけを入れると、同じことを `juiz-pkg` でできます(このクレートの版が雛形に入ります)。

```sh
cargo install juiz_pkg
juiz-pkg create my_robot -b rust
```

## ライセンス

MIT OR Apache-2.0
