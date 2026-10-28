# hsjuiz

juiz の Process / Container を Haskell で書くためのライブラリです(Linux / macOS、GHC 9.6 以降)。

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
| `Double`, `Float` | 小数(整数も受け取れる) |
| `Text` | 文字列 |
| `ByteString` | base64 の文字列 |
| `[a]` | 配列 |
| `Map Text a` | オブジェクト |
| `Maybe a` | null なら `Nothing` |
| `()` | null(結果のみ) |
| `Value` | どんな値でも(`JNull`, `JBool`, `JInt`, `JUInt`, `JFloat`, `JString`, `JArray`, `JObject`) |

オブジェクトを返すときは `object ["x" .= (1 :: Int64), "name" .= ("abc" :: Text)]` のように書けます。
自分の型は `fromValue :: Value -> Either Text a` / `toValue :: a -> Value` を書いてインスタンスにします。

### 型付きの引数・戻り値(IDL の型)

juiz の IDL(`.jidl`)で定義した型を使うと、引数・戻り値が型付きになります(manifest に型の情報が載り、
ほかの言語の Process と同じ型で受け渡しでき、接続のときに型が検査されます)。`juiz-idl -g haskell` で
Haskell のモジュールを作り(Template Haskell は使いません)、cabal の `hs-source-dirs` と `other-modules` に足します。

```sh
juiz-idl -g haskell -o gen idl/geometry.jidl   # gen/Robot/Geometry.hs など
```

```haskell
import Juiz
import Robot.Geometry   -- Pose2D (..)、Point2D (..)、MotorMode (..) など

moveManifest :: ProcessManifest
moveManifest = processManifest "pose_move_hs"
  & addTypedArgNoDefault (Proxy :: Proxy Pose2D) "pose" "current pose"
  & addFloatArg "dx" "" 1.0

poseMove :: Pose2D -> Double -> Pose2D
poseMove pose dx = pose { pose2DPosition = (pose2DPosition pose) { point2DX = point2DX (pose2DPosition pose) + dx } }

juizProcess 'poseMove 'moveManifest
```

- 型付きの引数は `addTypedArg 名前 説明 既定値`(既定値はその型の値)、`addTypedArgNoDefault (Proxy :: Proxy T) 名前 説明`(既定値なし)、
  `addTypedArgValue (Proxy :: Proxy T) 名前 説明 辞書の既定値` で足します。関数の引数が型付きの型なのにマニフェストが
  `addObjectArg` などの型なしの引数だと、juiz が読み込むときにエラーになります。
- 関数の結果が型付きの型なら、戻り値の型(`output_schema`)は自動で載ります(`returns (Proxy :: Proxy T)` で書いても同じ)。
- 生成した record のフィールドの関数は「型名 + フィールド名」(`pose2DPosition`、`frame_id` なら `pose2DFrameId`)、
  列挙の構築子は「型名 + 記号」(`MotorModeIdle`)です。すべてのフィールドに既定値がある record には `default<型名>` もあります。
- 引数は juiz が型で読んでから渡します(足りないフィールドは既定値で埋め、型にないキーは無視)。型に合わない値は
  `JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR`(-702)になります。戻り値は辞書にして返します。
- IDL を使わずに手で書いた型も、Avro のスキーマから `unsafeTypeInfo` で型の情報を作り、`JuizType`・`FromCapsule`・`ToCapsule`
  のインスタンス(`fromCapsuleType` / `toCapsuleType` で型の情報を返す)を書けば型付きにできます(`Juiz.Type` を参照)。

詳しくは juiz の docs/spec/datatype.md §22 にあります。

### エラー

関数の中の例外は C の呼び出し元(juiz)に出さず、エラーコードに変えて返します。内容は標準エラー出力に書きます。

- 引数が無い・型が合わない: `JUIZ_VALUE_NOT_FOUND_ERROR`(-102)・`JUIZ_VALUE_CONVERTER_ERROR`(-103)
- `throwJuiz code "説明"` で投げた `JuizError`: そのコード
- それ以外の例外(`error` など): `JUIZ_PROCESS_FUNCTION_NULL_OPT_RETURNED`(-501)

どの場合も juiz の呼び出しはエラーになり、プロセスは落ちません。

## Container を書く

Container は状態を持つもので、ContainerProcess はその状態を第 1 引数に受け取る関数です。
juiz の設定ファイルでは、Container と ContainerProcess はそれぞれ別の共有ライブラリになります
(1 つのライブラリにまとめるときは [Component](#component-にまとめる) にします)。

状態の型は、Container と ContainerProcess のどちらのライブラリからも同じモジュールを使ってそろえます。

```haskell
-- ExampleContainerState.hs (両方のライブラリの other-modules に入れる)
module ExampleContainerState (ExampleContainer (..)) where

import Data.IORef (IORef)
import Data.Int (Int64)

newtype ExampleContainer = ExampleContainer (IORef Int64)
```

```haskell
-- ExampleContainer.hs: Container (example_container_hs)
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
module ExampleContainer () where

import Juiz
import ExampleContainerState

manifest :: ContainerManifest
manifest = containerManifest "example_container_hs"
  & description "Holds an integer value."
  & addIntArg "value" "initial value" 0

create :: Int64 -> IO ExampleContainer
create value = ExampleContainer <$> newIORef value

juizContainer 'create 'manifest
```

```haskell
-- ExampleContainerIncrement.hs: ContainerProcess (example_container_hs_increment)
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
module ExampleContainerIncrement () where

import Juiz
import ExampleContainerState

manifest :: ProcessManifest
manifest = containerProcessManifest "example_container_hs_increment" "example_container_hs"
  & description "Adds arg0 to the value of the container."
  & addIntArg "arg0" "value to be added" 1

increment :: ExampleContainer -> Int64 -> IO Int64
increment (ExampleContainer ref) arg0 = atomicModifyIORef' ref (\v -> (v + arg0, v + arg0))

juizContainerProcess 'increment 'manifest
```

- `containerManifest` の引数(`addIntArg` など)は、**Container を作る関数の引数** になります。
  設定ファイルの `containers` に書いた値を受け取り、書いていなければ既定値になります。
- Container を作る関数の結果(`IO s` または `s`)が状態です。書き換える値は `IORef` などに入れてください
  (`Juiz` から `IORef` / `newIORef` / `readIORef` / `writeIORef` / `modifyIORef'` / `atomicModifyIORef'` も使えます)。
  juiz は同じ Container の ContainerProcess を同時には呼びません。
- `containerProcessManifest "型名" "Container の型名"` で ContainerProcess のマニフェストを作ります。
  引数は第 1 引数(状態)の後に続く引数の順です。
- 状態は `StablePtr` に包んで juiz に渡します。Container を削除すると解放され、GC に回収されます。
  デバイスを閉じるなど、削除のときにしたいことがあれば `juizContainerWithDestroy 'create 'destroy 'manifest`
  (`destroy :: s -> IO ()`)を使います。
- ContainerProcess の状態の型と Container の状態の型が違う(別の型の Haskell の Container や、C++ など別の言語の
  Container に結びつけた)ときは、呼び出しが `JUIZ_HASKELL_CONTAINER_TYPE_ERROR`(-602)のエラーになります。
  型は `Typeable` で得たモジュール名つきの型名(`ExampleContainerState.ExampleContainer` など)で比べるので、両方のライブラリで同じモジュールの同じ型を使ってください(Container と ContainerProcess はライブラリごとにパッケージの識別子が変わることがあるため、`cast` は使っていません)。

設定ファイルは C++ の Container と同じ形です。

```yaml
plugins:
  container_factories:
    example_container_hs:
      language: "haskell"
      path: "./target"
      processes:
        example_container_hs_increment:
          path: "./target"
containers:
  - type_name: "example_container_hs"
    name: "hscont0"
    value: 15
    processes:
      - type_name: "example_container_hs_increment"
        name: "hsinc0"
```

## Component にまとめる

Component は、1 つのライブラリに複数の Process / Container / ContainerProcess をまとめたものです(C++ の
`COMPONENT_PROCESS_FACTORY` などと同じ仕組み)。ライブラリが一つで済み、Container と ContainerProcess が
同じライブラリに入るので、状態の型を別のモジュールに分ける必要もありません。

要素の関数とマニフェストは、単体で公開するときと同じものを使います。`juizComponent` に Component の
マニフェストと要素の一覧を渡します。

```haskell
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
module ExampleComponent () where

import Juiz

component :: ComponentManifest
component = componentManifest "example_component_hs"
  & description "Haskell component example."

incrementManifest :: ProcessManifest
incrementManifest = processManifest "example_component_hs_increment"
  & addIntArg "arg0" "value to be incremented" 3

increment :: Int64 -> Int64
increment arg0 = arg0 + 1

newtype Counter = Counter (IORef Int64)

counterManifest :: ContainerManifest
counterManifest = containerManifest "example_component_hs_counter"
  & addIntArg "value" "initial value" 0

createCounter :: Int64 -> IO Counter
createCounter value = Counter <$> newIORef value

counterGetManifest :: ProcessManifest
counterGetManifest = containerProcessManifest "example_component_hs_counter_get" "example_component_hs_counter"

counterGet :: Counter -> IO Int64
counterGet (Counter ref) = readIORef ref

juizComponent 'component
  [ componentProcess 'increment 'incrementManifest
  , componentContainer 'createCounter 'counterManifest
  , componentContainerProcess 'counterGet 'counterGetManifest
  ]
```

- 要素は `componentProcess` / `componentContainer` / `componentContainerWithDestroy` / `componentContainerProcess` で
  書きます。引数は `juizProcess` などと同じです。別のモジュールに書いた関数とマニフェストも(公開して import すれば)使えます。
- `componentManifest` に渡す名前が、設定ファイルの `components` に書く名前とライブラリの名前になります。
- `juizComponent` は C++ と同じ名前の C の関数を生成します: マニフェストを返す `component_manifest_entry_point`、
  要素ごとの `<関数名>_factory_entry_point`(Container は `<関数名>_factory_destroy_entry_point` も)と
  `<関数名>_manifest_entry_point`。要素のマニフェストの `factory` は `<関数名>_factory` に付け直すので、自分で書く必要はありません。
  そのため **関数名は Component の中で重ならないように** してください(重なるとコンパイルエラーになります)。
- ContainerProcess の Container は同じ Component に入れます。Component に無い Container の ContainerProcess を入れたり、
  型名が重なっていたりすると、juiz が読み込むときにエラーになります。

設定ファイルでは `plugins` の `components` に書き、Process や Container は要素の型名で作ります。

```yaml
plugins:
  components:
    example_component_hs:
      language: "haskell"
      path: "./target"
processes:
  - type_name: "example_component_hs_increment"
    name: "hsinc0"
containers:
  - type_name: "example_component_hs_counter"
    name: "hscounter0"
    value: 15
    processes:
      - type_name: "example_component_hs_counter_get"
        name: "hsget0"
```

一つの juiz で複数の Haskell のライブラリ(単体の Process や Container、Component)を読み込むこともできます。

## ビルドする

Process / Container / ContainerProcess / Component ごとに `foreign-library` を一つ作ります。`-threaded` は必須です(juiz は Process をワーカースレッドから呼ぶため。
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

### libjuiz_sdk と hsjuiz を用意する

ビルドには、使う `juiz` と **同じ版** の libjuiz_sdk が必要です。版の違う libjuiz_sdk でビルドしたライブラリは、juiz が読み込むときにエラーにします。

libjuiz_sdk は、v0.0.26 から [GitHub Releases](https://github.com/OpenJUIZ/juiz/releases) に OS・CPU ごとのアーカイブ
(`juiz_sdk-<版>-<ターゲット名>.tar.gz`、Linux x86_64・aarch64 と macOS arm64・x86_64)を置いています。
`scripts/install-juiz-sdk.sh` で取得して `~/.juiz/sdk/<版>` に展開できます。

```terminal
curl -fsSL https://raw.githubusercontent.com/OpenJUIZ/juiz/main/scripts/install-juiz-sdk.sh | sh -s -- 0.0.26
```

それより前の版では、juiz のソースを同じ版のタグで取得し、`cargo build --release -p juiz_sdk` でできる `target/release` の libjuiz_sdk を使います。

hsjuiz は Hackage への公開を準備中です。それまでは、juiz のソースを `juiz` と同じ版のタグで取得し、その `bindings/hsjuiz` を
自分のパッケージの `cabal.project` に書いて使います。あわせて hsjuiz に libjuiz_sdk の場所を渡します。`ghc-options` の rpath は、
Template Haskell(`juizProcess` など)を実行するときに GHC が hsjuiz を読み込むのに必要です。
できたライブラリにも libjuiz_sdk の場所が rpath として入ります。

```cabal
packages: . /home/me/juiz-0.0.26/bindings/hsjuiz

package hsjuiz
  extra-lib-dirs: /home/me/.juiz/sdk/0.0.26/lib
  ghc-options: -optl-Wl,-rpath,/home/me/.juiz/sdk/0.0.26/lib
```

```terminal
git clone --depth 1 --branch v0.0.26 https://github.com/OpenJUIZ/juiz.git /home/me/juiz-0.0.26
cabal build all
```

Hackage で公開したあとは、`packages:` に hsjuiz のソースを書かずに、`cabal update` で Hackage から入れられるようになります。

juiz のリポジトリの中でビルドするときは、`cargo build` でできる `target/debug` の libjuiz_sdk を
`--extra-lib-dirs` で渡します(`examples/haskell/build.sh` がこの方法です)。

```terminal
cabal build all --extra-lib-dirs=<juiz>/target/debug --ghc-options="-optl-Wl,-rpath,<juiz>/target/debug"
```

macOS で、選んでいる Xcode より新しい Command Line Tools が入っている(たとえば Xcode 26 に Command Line Tools 27 の
MacOSX27.0.sdk)と、リンクが `tapi error: malformed file ... unknown architecture arm64e.x1-macos` で失敗します。
GHC はリンクのときに Command Line Tools の SDK(`/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk`)を `-L` で渡すので、
Xcode の古いリンカが新しい SDK の `.tbd` を読めないためです(issue #143)。Command Line Tools のリンカを使えば通ります。

```terminal
DEVELOPER_DIR=/Library/Developer/CommandLineTools cabal build all ...
```

Xcode を Command Line Tools と同じ版に上げても直ります。`examples/haskell/build.sh` はこの組み合わせを見つけると自動で `DEVELOPER_DIR` を切り替えます。

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
- hsjuiz の `juiz_hs_init` は、RTS を初期化する前に `setKeepCAFs()` を呼び、CAF(トップレベルの値)を GC で
  回収しないようにしています。2 つ目以降に開いたライブラリの `foreign export` が GC の根にならず、
  エントリポイントが回収されてプロセスが落ちるのを防ぐためです(issue #133)。

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

Container と ContainerProcess は `containerManifestEntryPoint` / `containerCreateEntryPoint` /
`containerDestroyEntryPoint` と `containerProcessManifestEntryPoint` / `containerProcessEntryPoint` で同じように書けます。

```haskell
hsManifest :: ManifestFunction
hsManifest = containerManifestEntryPoint manifest create
hsCreate :: ContainerCreateFunction
hsCreate = containerCreateEntryPoint manifest create
hsDestroy :: ContainerDestroyFunction
hsDestroy = containerDestroyEntryPoint manifest
hsFactory :: ContainerFactoryFunction
hsFactory = pure p_hsCreate

foreign export ccall "manifest_entry_point" hsManifest :: ManifestFunction
foreign export ccall "example_container_hs_create_entry_point" hsCreate :: ContainerCreateFunction
foreign import ccall "&example_container_hs_create_entry_point" p_hsCreate :: FunPtr ContainerCreateFunction
foreign export ccall "container_factory_entry_point" hsFactory :: ContainerFactoryFunction
foreign export ccall "container_factory_destroy_entry_point" hsDestroy :: ContainerDestroyFunction
```

Component は、要素ごとに上と同じ関数を `<関数名>_factory_entry_point` などの名前で公開し、
マニフェストの `factory` をその名前(`<関数名>_factory`)にしてから、`componentManifestEntryPoint` で
`component_manifest_entry_point` を書きます。`processPart` などは、要素のマニフェストと関数の引数の数を確かめるために使います。

```haskell
incrementManifest' :: ProcessManifest
incrementManifest' = incrementManifest & factory "increment_factory"

hsComponentManifest :: ManifestFunction
hsComponentManifest = componentManifestEntryPoint component
  [ processPart incrementManifest' increment
  , containerPart counterManifest' createCounter
  , containerProcessPart counterGetManifest' counterGet
  ]

foreign export ccall "component_manifest_entry_point" hsComponentManifest :: ManifestFunction
-- increment_factory_entry_point、createCounter の factory の entry_point と destroy_entry_point なども同じように公開する
```

公開する関数の決まりは `bindings/cjuiz/README.md` にあります。

## ライセンス

MIT OR Apache-2.0
