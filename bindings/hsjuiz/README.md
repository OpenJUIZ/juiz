# hsjuiz

juiz の Process を Haskell で書くためのライブラリです(Linux / macOS、GHC 9.6 以降)。

Haskell の関数を cabal の `foreign-library`(共有ライブラリ)として公開し、juiz が設定ファイルの
`language: "haskell"` のプラグインとして読み込みます。値の受け渡しには libjuiz_sdk の C 関数
(`bindings/cjuiz/include/juiz_sdk.h`)を使います。

## Process を書く

```haskell
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
module IncrementProcess () where

import Juiz

manifest :: ProcessManifest
manifest = processManifest "increment_process_hs"
  & description "Adds 1 to arg1."
  & addIntArg "arg1" "value to be incremented" 1

increment :: Int64 -> IO Int64
increment arg1 = pure (arg1 + 1)

juizProcess 'increment 'manifest
```

- `processManifest` に渡す名前が、設定ファイルの `type_name` とライブラリの名前になります。
- 引数は `addBoolArg` / `addIntArg` / `addFloatArg` / `addStringArg` / `addArrayArg` / `addObjectArg` で足します。
  **足した順が関数の引数の順** です。マニフェストの引数の数と関数の引数の数が違うと、juiz が読み込むときにエラーになります。
- `juizProcess 'func 'manifest` が、juiz が探す C の関数(`manifest_entry_point`、`process_factory_entry_point` など)を生成します。
- 関数の結果は `IO r` でも、IO の付かない `r` でもかまいません。型は具体的に書いてください(多相な関数は使えません)。
- 文字列の引数は `OverloadedStrings` を付けて `Text` で書きます。

### 使える型

引数の型は `FromCapsule`、結果の型は `ToCapsule` のインスタンスにします。用意してあるのは次の型です。

| Haskell | juiz の値 |
|---|---|
| `Bool` | 真偽値 |
| `Int64`, `Int`, `Int8`〜`Int32`, `Word8`〜`Word32` | 整数(範囲外ならエラー) |
| `Word64` | 0 以上の整数 |
| `Double` | 小数(整数も受け取れる) |
| `Text` | 文字列 |
| `[a]` | 配列 |
| `Map Text a` | オブジェクト |
| `Maybe a` | null なら `Nothing` |
| `()` | null(結果のみ) |
| `Value` | どんな値でも(`JNull`, `JBool`, `JInt`, `JUInt`, `JFloat`, `JString`, `JArray`, `JObject`) |

オブジェクトを返すときは `object ["x" .= (1 :: Int64), "name" .= ("abc" :: Text)]` のように書けます。
自分の型は `fromValue :: Value -> Either Text a` / `toValue :: a -> Value` を書いてインスタンスにします。

### エラー

関数の中の例外は C の呼び出し元(juiz)に出さず、エラーコードに変えて返します。内容は標準エラー出力に書きます。

- 引数が無い・型が合わない: `JUIZ_VALUE_NOT_FOUND_ERROR`(-102)・`JUIZ_VALUE_CONVERTER_ERROR`(-103)
- `throwJuiz code "説明"` で投げた `JuizError`: そのコード
- それ以外の例外(`error` など): `JUIZ_PROCESS_FUNCTION_NULL_OPT_RETURNED`(-501)

どの場合も juiz の呼び出しはエラーになり、プロセスは落ちません。

## ビルドする

Process ごとに `foreign-library` を一つ作ります。`-threaded` は必須です(juiz は Process をワーカースレッドから呼ぶため。
付け忘れると、juiz が読み込むときに `juiz_hs_init` が `JUIZ_HASKELL_NOT_THREADED_RTS_ERROR`(-601)を返してエラーになります)。

```cabal
foreign-library increment_process_hs
    type:             native-shared
    default-language: Haskell2010
    hs-source-dirs:   src
    other-modules:    IncrementProcess
    build-depends:    base, hsjuiz
    ghc-options:      -threaded
```

libjuiz_sdk(juiz のリポジトリで `cargo build` すると `target/debug` にできる)の場所を `--extra-lib-dirs` で渡します。

```terminal
cabal build all --extra-lib-dirs=<juiz>/target/debug --ghc-options="-optl-Wl,-rpath,<juiz>/target/debug"
```

できた `lib<名前>.so`(macOS は `.dylib`)を、設定ファイルの `path` のディレクトリに置きます。

```yaml
plugins:
  process_factories:
    increment_process_hs:
      language: "haskell"
      path: "./target"
```

- RTS(GHC のランタイム)は GHC の共有ライブラリで、同じプロセスに読み込んだ Haskell のライブラリすべてで共有されます。
  **一つの juiz で読み込む Haskell のライブラリは、すべて同じ GHC でビルドしてください。**
- RTS の初期化(`juiz_hs_init`)は hsjuiz が用意しているので、自分で書く必要はありません。
  GHC は `hs_exit` の後に初期化し直せないため、hsjuiz は `hs_exit` を呼ばず、`juiz_hs_exit` も用意していません。

例は `examples/haskell` にあります。

## Template Haskell を使わずに書く

`juizProcess` が生成するものは、`Juiz` の `manifestEntryPoint` と `processEntryPoint` を使って手でも書けます。
Process の関数の結果は `IO r` にしてください。

```haskell
{-# LANGUAGE OverloadedStrings #-}
module IncrementProcess () where

import Juiz

manifest :: ProcessManifest
manifest = processManifest "increment_process_hs"
  & description "Adds 1 to arg1."
  & addIntArg "arg1" "value to be incremented" 1

increment :: Int64 -> IO Int64
increment arg1 = pure (arg1 + 1)

hsManifest :: ManifestFunction
hsManifest = manifestEntryPoint manifest increment

hsProcess :: ProcessFunction
hsProcess = processEntryPoint manifest increment

hsFactory :: ProcessFactoryFunction
hsFactory = pure p_hsProcess

foreign export ccall "manifest_entry_point" hsManifest :: ManifestFunction
-- Process の関数そのもの。名前はほかのライブラリと重ならないものにする
foreign export ccall "increment_process_hs_process_entry_point" hsProcess :: ProcessFunction
foreign import ccall "&increment_process_hs_process_entry_point" p_hsProcess :: FunPtr ProcessFunction
foreign export ccall "process_factory_entry_point" hsFactory :: ProcessFactoryFunction
```

公開する関数の決まりは `bindings/cjuiz/README.md` にあります。

## ライセンス

MIT OR Apache-2.0
