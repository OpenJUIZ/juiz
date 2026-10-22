# cjuiz: C ABI プラグインのための C ヘッダ

`include/juiz_sdk.h` は、libjuiz_sdk(`juiz_sdk` クレートの cdylib)が公開している C の関数とエラーコードの宣言です。
C++ のヘッダ(`bindings/cppjuiz/include/juiz/`)もこのファイルを include しています。
C の関数を呼べて、C の関数を公開できる言語(C、C++、Haskell の FFI など)なら、このヘッダを元に juiz のプラグインを書けます。

## プラグインが公開する関数

juiz は設定ファイルの `language: "c++"` のプラグイン(共有ライブラリ)を開き、次の名前の関数を探します。
どれも C の呼出規約(`extern "C"`)で公開してください。`<シンボル名>` は、単体の Process / Container なら下の表の名前、
Component の中の要素ならマニフェストの `factory` に書いた名前です。

| 関数 | 型 | 役割 |
|---|---|---|
| `manifest_entry_point`(Component では `component_manifest_entry_point`) | `int64_t (capsule_ptr*)` | マニフェストを capsule_ptr に書き込む |
| `<シンボル名>_entry_point`(Process は `process_factory`) | `int64_t (*(void))(capsule_map*, capsule*)` | Process の関数を返す |
| `<シンボル名>_entry_point`(Container は `container_factory`) | `int64_t (*(void))(capsule_map*, void**)` | Container を作る関数を返す。作った Container を `void*` で返す |
| `<シンボル名>_destroy_entry_point`(Container は `container_factory_destroy_entry_point`) | `void (void*)` | 任意。Container を削除するときに呼ばれ、Container を解放する。無ければ何もしない |
| `<シンボル名>_entry_point`(ContainerProcess は `container_process_factory`) | `int64_t (*(void))(void*, capsule_map*, capsule*)` | ContainerProcess の関数を返す。第1引数が Container |

関数の戻り値は成功なら `JUIZ_OK`(0)、失敗なら負の値です。C++ では `bindings/cppjuiz/include/juiz/manifest.h` のマクロ
(`PROCESS_FACTORY`、`CONTAINER_FACTORY` など)がこれらの関数を生成します。
