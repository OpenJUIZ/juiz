# libjuiz_sdk のアーカイブ (juiz_sdk-<版>-<ターゲット名>) を CMake の find_package で使うための設定 (issue #162, #273)。
# scripts/package-juiz-sdk.sh がアーカイブの lib/cmake/juiz_sdk/ に置く。
#
#   find_package(juiz_sdk 0.0.27 EXACT CONFIG REQUIRED)  # CMAKE_PREFIX_PATH にアーカイブを展開したディレクトリを足す
#   add_library(my_process SHARED my_process.cpp)
#   target_link_libraries(my_process PRIVATE juiz_sdk::juiz_sdk)
#   juiz_sdk_copy_runtime(my_process)                     # Windows ではプラグインの隣に juiz_sdk.dll を写す
#
# 定義するもの:
#   juiz_sdk::juiz_sdk           libjuiz_sdk と C / C++ のヘッダ (cjuiz/include, cppjuiz/include)。C++17 以上
#   juiz_sdk_copy_runtime(<target>)
#                                Windows: ビルドした <target> の隣に juiz_sdk.dll を写す。juiz はプラグインの
#                                ディレクトリから juiz_sdk.dll を探す (Linux の rpath $ORIGIN に当たる)。
#                                Linux / macOS では何もしない (プラグインの rpath に lib/ が入る)
#   juiz_generate_types(...)     juiz-idl で IDL から C++ の型を作る (cppjuiz/cmake/juiz_idl.cmake)
#   juiz_sdk_VERSION, juiz_sdk_ROOT_DIR, juiz_sdk_RUNTIME_LIBRARY (libjuiz_sdk の .so / .dylib / .dll)
#
# juiz はプラグインが読み込んだ libjuiz_sdk の版が自分と違うと読み込まないので、juiz と同じ版を使う。

get_filename_component(juiz_sdk_ROOT_DIR "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)

if (WIN32)
  set(juiz_sdk_RUNTIME_LIBRARY "${juiz_sdk_ROOT_DIR}/bin/juiz_sdk.dll")
  set(_juiz_sdk_implib "${juiz_sdk_ROOT_DIR}/lib/juiz_sdk.dll.lib")
elseif (APPLE)
  set(juiz_sdk_RUNTIME_LIBRARY "${juiz_sdk_ROOT_DIR}/lib/libjuiz_sdk.dylib")
else ()
  set(juiz_sdk_RUNTIME_LIBRARY "${juiz_sdk_ROOT_DIR}/lib/libjuiz_sdk.so")
endif ()

foreach (_juiz_sdk_file IN ITEMS "${juiz_sdk_RUNTIME_LIBRARY}" "${_juiz_sdk_implib}")
  if (_juiz_sdk_file AND NOT EXISTS "${_juiz_sdk_file}")
    set(${CMAKE_FIND_PACKAGE_NAME}_FOUND FALSE)
    set(${CMAKE_FIND_PACKAGE_NAME}_NOT_FOUND_MESSAGE
      "${_juiz_sdk_file} がありません。この OS・CPU 向けの juiz_sdk のアーカイブか確かめてください。")
    return()
  endif ()
endforeach ()

if (NOT TARGET juiz_sdk::juiz_sdk)
  add_library(juiz_sdk::juiz_sdk SHARED IMPORTED)
  set_target_properties(juiz_sdk::juiz_sdk PROPERTIES
    IMPORTED_LOCATION "${juiz_sdk_RUNTIME_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${juiz_sdk_ROOT_DIR}/cjuiz/include;${juiz_sdk_ROOT_DIR}/cppjuiz/include"
    INTERFACE_COMPILE_FEATURES cxx_std_17
    # ヘッダは日本語のコメントを含む BOM なしの UTF-8。MSVC は既定でシステムのコードページで読むので、
    # 日本語の Windows (CP932) で壊れないよう /utf-8 を付ける
    INTERFACE_COMPILE_OPTIONS "$<$<COMPILE_LANG_AND_ID:CXX,MSVC>:/utf-8>;$<$<COMPILE_LANG_AND_ID:C,MSVC>:/utf-8>")
  if (WIN32)
    set_target_properties(juiz_sdk::juiz_sdk PROPERTIES IMPORTED_IMPLIB "${_juiz_sdk_implib}")
  elseif (APPLE)
    # install name は @rpath/libjuiz_sdk.dylib (package-juiz-sdk.sh で付け替えている)
    set_target_properties(juiz_sdk::juiz_sdk PROPERTIES IMPORTED_SONAME "@rpath/libjuiz_sdk.dylib")
  else ()
    # rustc の cdylib には SONAME が無い。そのままだとプラグインに libjuiz_sdk.so の絶対パスが書き込まれるので、
    # -ljuiz_sdk でリンクさせる (探す場所はプラグインの rpath)
    set_target_properties(juiz_sdk::juiz_sdk PROPERTIES IMPORTED_NO_SONAME TRUE)
  endif ()
endif ()

function(juiz_sdk_copy_runtime TARGET)
  if (WIN32)
    add_custom_command(TARGET ${TARGET} POST_BUILD
      COMMAND ${CMAKE_COMMAND} -E copy_if_different "${juiz_sdk_RUNTIME_LIBRARY}" "$<TARGET_FILE_DIR:${TARGET}>"
      VERBATIM)
  endif ()
endfunction()

include("${juiz_sdk_ROOT_DIR}/cppjuiz/cmake/juiz_idl.cmake")

unset(_juiz_sdk_file)
unset(_juiz_sdk_implib)
