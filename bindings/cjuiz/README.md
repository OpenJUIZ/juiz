# cjuiz: C ABI プラグインのための C ヘッダ

`include/juiz_sdk.h` は、libjuiz_sdk(`juiz_sdk` クレートの cdylib)が公開している C の関数とエラーコードの宣言です。
C++ のヘッダ(`bindings/cppjuiz/include/juiz/`)もこのファイルを include しています。
C の関数を呼べて、C の関数を公開できる言語(C、C++、Haskell の FFI など)なら、このヘッダを元に juiz のプラグインを書けます。

## libjuiz_sdk の入手

v0.0.26 以降は、libjuiz_sdk とこのヘッダ(C++ のヘッダも)を OS・CPU ごとのアーカイブ `juiz_sdk-<版>-<ターゲット名>.tar.gz` にして
[GitHub Releases](https://github.com/OpenJUIZ/juiz/releases) に置いています(Linux x86_64・aarch64、macOS arm64・x86_64)。
`scripts/install-juiz-sdk.sh <版>` で取得して `~/.juiz/sdk/<版>` に展開できます。中身は次のとおりで、pkg-config の `juiz_sdk.pc` も入っています。

0.0.26 の次の版からは、Windows x86_64 向けの `juiz_sdk-<版>-x86_64-pc-windows-msvc.zip` も置き、
どのアーカイブにも CMake の `find_package(juiz_sdk)` 用の `juiz_sdkConfig.cmake` を入れます(issue #273、#162)。
Windows では PowerShell で `scripts/install-juiz-sdk.ps1 <版>` を実行すると `%USERPROFILE%\.juiz\sdk\<版>` に展開できます。

```
lib/libjuiz_sdk.so (macOS は .dylib、install name は @rpath/libjuiz_sdk.dylib)
lib/pkgconfig/juiz_sdk.pc
bin/juiz_sdk.dll, lib/juiz_sdk.dll.lib (Windows は .so と .pc の代わりに DLL とインポートライブラリ)
lib/cmake/juiz_sdk/juiz_sdkConfig.cmake, juiz_sdkConfigVersion.cmake
cjuiz/include/juiz_sdk.h
cppjuiz/include/juiz/*.h
cppjuiz/cmake/juiz_idl.cmake
```

### CMake で使う

展開したディレクトリを `CMAKE_PREFIX_PATH` に足して(`-DCMAKE_PREFIX_PATH=<展開先>`)、次のように書きます。
`juiz_sdk::juiz_sdk` をリンクすると、C / C++ のヘッダの include パスと C++17 が付き、MSVC には `/utf-8` が付きます(ヘッダが UTF-8 の日本語のコメントを含むため)。

```cmake
find_package(juiz_sdk 0.0.27 EXACT CONFIG REQUIRED)   # 版は juiz と同じものを指定する
add_library(my_process SHARED my_process.cpp)
target_link_libraries(my_process PRIVATE juiz_sdk::juiz_sdk)
juiz_sdk_copy_runtime(my_process)   # Windows ではプラグインの隣に juiz_sdk.dll を写す (Linux / macOS では何もしない)
```

IDL から C++ の型を作る `juiz_generate_types()`(`cppjuiz/cmake/juiz_idl.cmake`)も `find_package` で使えるようになります。
Linux / macOS では、CMake がビルドしたプラグインの rpath にアーカイブの `lib/` を入れます。

### Windows の DLL の探し方

Windows では、プラグインの DLL は libjuiz_sdk の DLL `juiz_sdk.dll` に依存します。juiz はプラグインを
`LOAD_WITH_ALTERED_SEARCH_PATH` で開くので、`juiz_sdk.dll` を **プラグインの DLL と同じディレクトリ** に置けば見つかります
(Linux の rpath `$ORIGIN` に当たります)。PATH に通したディレクトリからも探します。
MSVC でビルドした DLL を 1 種類だけ配っていますが、境界は C ABI なので、MinGW など他のコンパイラで作ったプラグインからも使えます
(MinGW では `juiz_sdk.dll` を直接リンクできます)。

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

## データ型とネイティブ値(`juiz_type_*`、`capsule_set_native` など)

Process の引数・戻り値の型(docs/spec/datatype.md §21)のための関数です。`juiz_type_new` は Avro のスキーマから型を作り、
`capsule_ptr_read_typed_value` は引数を型で読みます(指紋と読み方は Rust の実装と同じ)。
`capsule_set_native` は C++ の値を辞書にせずに戻り値に入れ、`capsule_ptr_borrow_native` は型タグの同じ値をそのまま貸します
(遅延評価 L3、docs/spec/lazy-evaluation.md)。C++ では `bindings/cppjuiz/include/juiz/types.h` と `juiz-idl -g cpp` の生成コードがこれらを使います。

## 画像(`juiz_image`、`capsule_ptr_lock_as_image`、`capsule_set_image`)

Process の引数・戻り値の画像(Rust の `Capsule::Image`)を読み書きする関数です(docs/spec/large-data.md §10)。
`capsule_ptr_lock_as_image` は引数の画像を複製せずに callback の間だけ貸し、`capsule_set_image` は画像を複製して戻り値に入れます
(行の余白を取り除き、`JUIZ_IMAGE_FORMAT_BGR8` / `BGRA8` は RGB に並べ替えます)。C++ では `bindings/cppjuiz/include/juiz/image.h` の
`juiz::Image`(OpenCV との変換は `juiz/opencv.h`)、Haskell では hsjuiz の `Juiz.Image` がこれらを使います。

## Haskell のプラグイン(`language: "haskell"`)

読み込み方は C++ と同じで、上の関数に加えて次の関数が必要です。

| 関数 | 型 | 役割 |
|---|---|---|
| `juiz_hs_init` | `int64_t (void)` | GHC のランタイム(RTS)を初期化する(中で `hs_init` を呼ぶ)。成功なら 0。juiz はライブラリを開いた直後、マニフェストより先に呼ぶ。無ければ読み込みはエラーになる |

Haskell のライブラリ hsjuiz(`bindings/hsjuiz`)を使うと、これらの関数は hsjuiz が用意します(`juiz_hs_init` は C のスタブ、ほかは `juizProcess` / `juizContainer` / `juizContainerProcess` が生成)。

- juiz は `juiz_hs_init` を一度に一つずつ呼びます(`hs_init` はスレッド安全ではないため)。
- GHC は `hs_exit` の後に `hs_init` し直すことができないので、juiz は `hs_exit` を呼ばず、Haskell のライブラリを閉じません。RTS はプロセスの終了とともに片付きます。
- juiz は Process をワーカースレッドから呼ぶので、`-threaded` の RTS でビルドしてください。hsjuiz の `juiz_hs_init` は、`-threaded` でなければ初期化せずに `JUIZ_HASKELL_NOT_THREADED_RTS_ERROR`(-601)を返します。
- Linux / macOS では、GHC の静的ライブラリは位置独立コード(`-fPIC`)ではないため、RTS を静的に抱えた共有ライブラリは作れません。cabal の `foreign-library` は RTS と base を GHC の共有ライブラリ(`libHSrts_thr` など)にリンクするので、同じプロセスに読み込んだ Haskell のライブラリは一つの RTS を共有します。そのため **一つの juiz で読み込む Haskell のライブラリは、すべて同じ GHC でビルドしてください**。
- Windows では逆に、GHC が Haskell の共有ライブラリを持たないので、DLL ごとに RTS を静的に抱えます(`options: standalone`)。DLL ごとに RTS が別になるため、ある DLL の Container の `void*` を別の DLL の ContainerProcess に渡すと、hsjuiz は `JUIZ_HASKELL_CONTAINER_TYPE_ERROR`(-602)を返します(Container の領域に、作った RTS の目印を入れて比べています)。
- Haskell の例外を C の呼び出し元に出すとプロセスが落ちるので、公開する関数の中で捕まえてエラーコードに変えてください。
- `juiz_hs_init` を自分で書くときは、`hs_init` の前に `setKeepCAFs()`(`Rts.h`)を呼んでください。GHC は `foreign export` した関数を GC の根に登録する処理を `hs_init` の中でしか行わないので、`hs_init` の後に開いた 2 つ目以降のライブラリでは、foreign export した CAF が major GC で回収され、次に呼んだときにプロセスが落ちます。
- Container の `void*` には、hsjuiz は先頭に目印を置いた領域(中身は `StablePtr`)を渡します。hsjuiz の ContainerProcess は、目印が無い(別の言語の Container)か状態の型が違うときに `JUIZ_HASKELL_CONTAINER_TYPE_ERROR`(-602)を返します。

hsjuiz で書いた例が `examples/haskell` にあります。
