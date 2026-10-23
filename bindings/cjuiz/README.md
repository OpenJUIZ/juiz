# cjuiz: C ABI プラグインのための C ヘッダ

`include/juiz_sdk.h` は、libjuiz_sdk(`juiz_sdk` クレートの cdylib)が公開している C の関数とエラーコードの宣言です。
C++ のヘッダ(`bindings/cppjuiz/include/juiz/`)もこのファイルを include しています。
C の関数を呼べて、C の関数を公開できる言語(C、C++、Haskell の FFI など)なら、このヘッダを元に juiz のプラグインを書けます。

## libjuiz_sdk の入手

v0.0.22 以降は、libjuiz_sdk とこのヘッダ(C++ のヘッダも)を OS・CPU ごとのアーカイブ `juiz_sdk-<版>-<ターゲット名>.tar.gz` にして
[GitHub Releases](https://github.com/OpenJUIZ/juiz/releases) に置いています(Linux x86_64・aarch64、macOS arm64・x86_64)。
`scripts/install-juiz-sdk.sh <版>` で取得して `~/.juiz/sdk/<版>` に展開できます。中身は次のとおりで、pkg-config の `juiz_sdk.pc` も入っています。

```
lib/libjuiz_sdk.so (macOS は .dylib、install name は @rpath/libjuiz_sdk.dylib)
lib/pkgconfig/juiz_sdk.pc
cjuiz/include/juiz_sdk.h
cppjuiz/include/juiz/*.h
```

使う `juiz` と **同じ版** の libjuiz_sdk でビルドしてください。juiz はプラグインを開いたとき、プラグインが読み込んだ libjuiz_sdk の
`juiz_sdk_build_info()` を自分の juiz_sdk と比べ、版が違えば読み込みをエラーにします(Rust コンパイラの版だけが違うときは警告)。
libjuiz_sdk の Capsule などの構造体は juiz と同じでなければならないためです。

## プラグインが公開する関数

juiz は設定ファイルの `language: "c++"` または `language: "haskell"` のプラグイン(共有ライブラリ)を開き、次の名前の関数を探します。
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

## Haskell のプラグイン(`language: "haskell"`)

読み込み方は C++ と同じで、上の関数に加えて次の関数が必要です。

| 関数 | 型 | 役割 |
|---|---|---|
| `juiz_hs_init` | `int64_t (void)` | GHC のランタイム(RTS)を初期化する(中で `hs_init` を呼ぶ)。成功なら 0。juiz はライブラリを開いた直後、マニフェストより先に呼ぶ。無ければ読み込みはエラーになる |

Haskell のライブラリ hsjuiz(`bindings/hsjuiz`)を使うと、これらの関数は hsjuiz が用意します(`juiz_hs_init` は C のスタブ、ほかは `juizProcess` / `juizContainer` / `juizContainerProcess` が生成)。

- juiz は `juiz_hs_init` を一度に一つずつ呼びます(`hs_init` はスレッド安全ではないため)。
- GHC は `hs_exit` の後に `hs_init` し直すことができないので、juiz は `hs_exit` を呼ばず、Haskell のライブラリを閉じません。RTS はプロセスの終了とともに片付きます。
- juiz は Process をワーカースレッドから呼ぶので、`-threaded` の RTS でビルドしてください。hsjuiz の `juiz_hs_init` は、`-threaded` でなければ初期化せずに `JUIZ_HASKELL_NOT_THREADED_RTS_ERROR`(-601)を返します。
- GHC の静的ライブラリは位置独立コード(`-fPIC`)ではないため、RTS を静的に抱えた共有ライブラリは作れません。cabal の `foreign-library` は RTS と base を GHC の共有ライブラリ(`libHSrts_thr` など)にリンクするので、同じプロセスに読み込んだ Haskell のライブラリは一つの RTS を共有します。そのため **一つの juiz で読み込む Haskell のライブラリは、すべて同じ GHC でビルドしてください**。
- Haskell の例外を C の呼び出し元に出すとプロセスが落ちるので、公開する関数の中で捕まえてエラーコードに変えてください。
- `juiz_hs_init` を自分で書くときは、`hs_init` の前に `setKeepCAFs()`(`Rts.h`)を呼んでください。GHC は `foreign export` した関数を GC の根に登録する処理を `hs_init` の中でしか行わないので、`hs_init` の後に開いた 2 つ目以降のライブラリでは、foreign export した CAF が major GC で回収され、次に呼んだときにプロセスが落ちます。
- Container の `void*` には、hsjuiz は先頭に目印を置いた領域(中身は `StablePtr`)を渡します。hsjuiz の ContainerProcess は、目印が無い(別の言語の Container)か状態の型が違うときに `JUIZ_HASKELL_CONTAINER_TYPE_ERROR`(-602)を返します。

hsjuiz で書いた例が `examples/haskell` にあります。
