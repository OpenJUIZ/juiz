# hsjuiz の変更履歴

hsjuiz の版は juiz(crates.io の juiz_app、PyPI の pyjuiz / juiz-cli)と同じ番号にそろえています。
使う libjuiz_sdk も同じ版にしてください。

## 0.3.5

- hsjuiz の API は 0.3.4 から変えていない(juiz pkg の設計文書 juizdesign.yaml 対応 #592 の公開に版をそろえた)。

## 0.3.4

- `Juiz.Pose` に `setModel` / `clearModel` を足した。ContainerProcess の中から、その Container の 3D モデルを差し替える・外す
  (docs/spec/container-model.md §13.2、juiz#564)。値の誤りは `juizContainerModelInvalidError`(-908)。
- `Juiz.Capsule` から `fill`・`withCallback`・`withLockCallback` を出した(`setModel` が使う)。

## 0.3.3

- hsjuiz の API は 0.3.2 から変えていない(juiz の Zig 対応 #571 の公開に版をそろえた)。

## 0.3.2

- hsjuiz の API は 0.3.1 から変えていない(juiz 本体の Inlet の並行 push での panic 修正 #566 と、管理画面の 3D に副系・親系の Anchor を出す #567 の公開に版をそろえた)。

## 0.3.1

- hsjuiz の API は 0.3.0 から変えていない(juiz 本体の URDF / SDF の取り込み juiz import #388 と、実行中のモデルの差し替え #389 の公開に版をそろえた)。C ABI からのモデルの差し替えは juiz#564 で予定。

## 0.3.0

- hsjuiz の API は 0.2.11 から変えていない(juiz 本体の .juizapp の使い勝手 #390 と、つながった System を全部たどる再帰の一覧・管理画面 #559 の公開に版をそろえた)。

## 0.2.11

- hsjuiz の API は 0.2.10 から変えていない(juiz 本体の Manifest / Profile の名前の整理 #503 などの公開に版をそろえた)。

## 0.2.10

- `model` / `modelPath` で Container の型の既定の 3D モデルを書けるようにした(`ContainerManifest` に `cmModel` を足した、#509)。

## 0.2.9

- hsjuiz の API は 0.2.8 から変えていない(Container の 3D モデル #379 と Zenoh ブローカー #487 の公開に版をそろえた。モデルは conf で書くので hsjuiz 側の追加はない)。

## 0.2.8

- hsjuiz の API は 0.2.7 から変えていない(juiz 本体の不具合修正 #449・#450・#479 と、pyjuiz の call_partial #83 に版をそろえた)。

## 0.2.7

- hsjuiz の API は 0.2.6 から変えていない(juiz pkg create --idl の Haskell 対応 #468 と、Hackage 方式の導入手順 #360 に版をそろえた)。

## 0.2.6

- hsjuiz の API は 0.2.5 から変えていない(juiz 本体の雛形生成コマンド juiz pkg create / add #455 に版をそろえた)。

## 0.2.5

- hsjuiz の API は 0.2.4 から変えていない(juiz 本体の juiz run --remap・--depend 名前=none #452、CLI のヘルプ文言 #447 に版をそろえた)。

## 0.2.4

- ContainerProcess の中から自分以外の Container の位置姿勢を読む `lookupContainerPose` を足した(issue #409)。
- ContainerProcess の中から Transform の値を書く `setTransform` と、`ContainerTransform`・`identityTransform`・
  `transformToValue`・`transformStampedToValue` を足した。書けないときは `juizTransformNotWritableError`、
  Transform が無いときは `juizTransformNotFoundError`(issue #410)。

## 0.2.3

- hsjuiz の API は 0.2.2 から変えていない(juiz 本体の不具合修正 #400〜#437 に版をそろえた)。
  Windows で readonly の ContainerProcess を公開する例の .def の書き忘れを直した(#435)。

## 0.2.2

- readonly の ContainerProcess を書けるようにした。manifest に `& readonly True` を付けると、juiz は Container を
  読み出しロックで呼び、同じ Container の readonly 同士は並行に走る。中で `setContainerPose` を呼ぶと
  `juizPoseNotWritableError`(Transformations Step 11、issue #377)。`ProcessManifest` に `pmReadonly` を足した。

## 0.2.1

- hsjuiz の API は 0.2.0 から変えていない(juiz 本体の System をまたぐ Transform #376 に版をそろえた)。

## 0.2.0

- ContainerProcess の中から Container の位置姿勢を読み書きする `Juiz.Pose`(`containerPose`・`setContainerPose`・
  `poseToValue`・`poseStampedToValue`)とエラーコード `juizTransformationsNotAttachedError` など(-901〜-905)を足した
  (Transformations Step 5、issue #372)。

## 0.1.0

- 0.0 系を締めくくる版。hsjuiz の API は 0.0.30 から変えていない(juiz 本体の Windows の Haskell DLL の修正 #262、libjuiz_sdk の版照合 #344、HTTP ブローカーの並行化 #356・#362 などに版をそろえた)。

## 0.0.30

- 対応する GHC を全 OS で 9.6 以降にそろえた。`tested-with` から 9.4.7 を外し、`base` の下限を 4.18(GHC 9.6)に上げた(issue #310)。
- hsjuiz のほかの変更はない(juiz 本体の Rust プラグインの C ABI 化 #291 などに版をそろえた)。

## 0.0.29

- hsjuiz 自体の変更はない(juiz 本体の修正 #301・#303・#304 に版をそろえた)。

## 0.0.28

- アプリ配布形式 `.juizapp` に Haskell のプラグインを入れるため、`scripts/bundle-plugin-deps.sh` で
  プラグインと libjuiz_sdk・GHC の共有ライブラリ・libffi・libgmp を 1 つのディレクトリに集められるようにした(issue #287)。

## 0.0.27

- Windows でも、Haskell のプラグインを DLL として作れるようにした(issue #137。手順の整備は #137 で続ける)。
- libjuiz_sdk の Windows(x86_64、MSVC)向けアーカイブ `juiz_sdk-<版>-x86_64-pc-windows-msvc.zip` を配るようにした(issue #273)。

## 0.0.26

- libjuiz_sdk を GitHub Releases のアーカイブ(`juiz_sdk-<版>-<ターゲット名>.tar.gz`)で配るようにした(issue #136)。
  Hackage での公開は準備中で、それまでは juiz のソース(`bindings/hsjuiz`)から使う。
- juiz は、プラグインが読み込んだ libjuiz_sdk の版が自分と違うと読み込みをエラーにする。
- 画像を引数・戻り値に使える(`Juiz.Image` の `Image`、`addImageArg`、issue #253)。IPC のブローカー越しにも生バイトで渡る。

## 0.0.25 まで

- Process(`juizProcess`)、Container / ContainerProcess(`juizContainer` / `juizContainerProcess`)、
  Component(`juizComponent`)を Haskell で書ける(issue #132〜#134)。
- IDL の型を引数・戻り値に使える(`Juiz.Type`、`addTypedArg` など、0.0.23 から、issue #196)。
