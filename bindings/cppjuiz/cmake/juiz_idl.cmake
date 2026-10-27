# juiz-idl で JIDL / .avsc から C++ の型のヘッダを作る CMake の関数 (issue #195、docs/spec/datatype.md §21)
#
#   include(<juiz>/bindings/cppjuiz/cmake/juiz_idl.cmake)
#   juiz_generate_types(<target>
#       IDL idl/geometry.jidl [...]          # 読む .jidl / .avsc (CMakeLists.txt からの相対パスでよい)
#       [DEPENDS idl/common.jidl ...]        # import した先など、変わったら作り直すファイル
#       [IMPORT_DIRS <dir> ...]              # import の探し先 (juiz-idl の -I、ほかの型パッケージの .jidl の置き場所)
#       [OUTPUT_DIR <dir>])                  # 既定は ${CMAKE_CURRENT_BINARY_DIR}/juiz_idl
#
# 標準の型 juiz_std は juiz-idl に組み込まれているので、IDL に juiz_std/geometry.jidl と書くか、
# .jidl で import idl "juiz_std/geometry.jidl"; と書けば使える (docs/spec/datatype.md §24)。
#
# ビルドのときに `juiz-idl -g cpp` を実行して <dir> に <namespace>.h と juiz_idl.h を書き、
# <dir> を <target> の include ディレクトリに足す。ソースからは #include "juiz_idl.h" で使う。
#
# juiz-idl は JUIZ_IDL_EXECUTABLE (キャッシュ変数) か PATH から探す。
# 入っていなければ `cargo install juiz_idl` で入れるか、juiz のコマンドの `juiz idl` を
# JUIZ_IDL_EXECUTABLE に指定する (`juiz idl` は juiz-idl と同じ引数を取る)。

function(juiz_generate_types TARGET)
  cmake_parse_arguments(JUIZ_IDL "" "OUTPUT_DIR" "IDL;DEPENDS;IMPORT_DIRS" ${ARGN})
  if (NOT JUIZ_IDL_IDL)
    message(FATAL_ERROR "juiz_generate_types(${TARGET}): IDL にファイルを指定してください")
  endif()
  if (NOT JUIZ_IDL_OUTPUT_DIR)
    set(JUIZ_IDL_OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/juiz_idl")
  endif()
  if (NOT JUIZ_IDL_EXECUTABLE)
    find_program(JUIZ_IDL_EXECUTABLE NAMES juiz-idl HINTS ${JUIZ_IDL_HINTS})
  endif()
  if (NOT JUIZ_IDL_EXECUTABLE)
    message(FATAL_ERROR "juiz-idl が見つかりません。`cargo install juiz_idl` で入れるか、"
      "-DJUIZ_IDL_EXECUTABLE=<juiz-idl のパス> を指定してください")
  endif()

  set(idl_files)
  set(idl_deps)
  foreach(f IN LISTS JUIZ_IDL_IDL)
    get_filename_component(abs "${f}" ABSOLUTE)
    if (EXISTS "${abs}")
      list(APPEND idl_files "${abs}")
      list(APPEND idl_deps "${abs}")
    else()
      # 組み込みの型 (juiz_std/…) はファイルがないので、書いたまま juiz-idl に渡す
      list(APPEND idl_files "${f}")
    endif()
  endforeach()
  set(import_args)
  foreach(d IN LISTS JUIZ_IDL_IMPORT_DIRS)
    get_filename_component(abs "${d}" ABSOLUTE)
    list(APPEND import_args -I "${abs}")
  endforeach()
  set(dep_files)
  foreach(f IN LISTS JUIZ_IDL_DEPENDS)
    get_filename_component(abs "${f}" ABSOLUTE)
    list(APPEND dep_files "${abs}")
  endforeach()

  set(header "${JUIZ_IDL_OUTPUT_DIR}/juiz_idl.h")
  # juiz-idl は中身の変わらないファイルを書き直さないので、最後に juiz_idl.h の時刻を進めて作り直しを終わらせる
  add_custom_command(
    OUTPUT "${header}"
    COMMAND "${JUIZ_IDL_EXECUTABLE}" -g cpp -o "${JUIZ_IDL_OUTPUT_DIR}" ${import_args} ${idl_files}
    COMMAND "${CMAKE_COMMAND}" -E touch "${header}"
    DEPENDS ${idl_deps} ${dep_files} "${JUIZ_IDL_EXECUTABLE}"
    COMMENT "juiz-idl: C++ の型を作る (${JUIZ_IDL_IDL})"
    VERBATIM)
  target_sources(${TARGET} PRIVATE "${header}")
  target_include_directories(${TARGET} PRIVATE "${JUIZ_IDL_OUTPUT_DIR}")
endfunction()
