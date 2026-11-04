# hsjuiz の変更履歴

hsjuiz の版は juiz(crates.io の juiz_app、PyPI の pyjuiz / juiz-cli)と同じ番号にそろえています。
使う libjuiz_sdk も同じ版にしてください。

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
