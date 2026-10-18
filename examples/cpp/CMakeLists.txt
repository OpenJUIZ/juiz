cmake_minimum_required(VERSION 3.13)

project(juiz_cpp_examples CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(JUIZ_HEADER_DIR bindings/cppjuiz/include)
set(PROJECT_HOME ${PROJECT_SOURCE_DIR}/../../)
include_directories(${PROJECT_HOME}/${JUIZ_HEADER_DIR})

link_directories(${PROJECT_HOME}/target/debug)

# WindowsだとRustが出力するDLLファイルのリンクライブラリは.dll.libという拡張子を持っているので、設定を変える必要がある。
# Mac, Linuxだとsoやdylibをリンクできるが、Windowsはdllではなくdllのエントリをもつlibをリンクする必要があるので。
if (WIN32)
set(CMAKE_LINK_LIBRARY_SUFFIX .dll.lib)
endif (WIN32)

# プロセスを宣言するときの操作を関数にしておく。
# まとめて変更できるように。
# コンテナやコンポーネントも全く同じ操作だけど、一応、別にしておく。
# 出力先はジェネレータ式 $<1:...> で書く。Visual Studio のような複数構成のジェネレータは
# 出力先に Debug/ などの構成名のディレクトリを足すが、ジェネレータ式を使うと足さなくなるので、
# どのプラットフォームでも ../target(コンポーネントは ../target/debug)に出力される。
function(add_juiz_process ARG_NAME)
  message("-- Adding JUIZ process (name='${ARG_NAME}')")
  cmake_parse_arguments(JUIZ "" "" "SOURCES" ${ARGN})
  add_library(${ARG_NAME} SHARED ${JUIZ_SOURCES})
  target_link_libraries(${ARG_NAME} juiz_sdk)
  set_target_properties(${ARG_NAME}
      PROPERTIES
      ARCHIVE_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/>"
      LIBRARY_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/>"
      RUNTIME_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/>"
  )
endfunction()


function(add_juiz_container ARG_NAME)
  message("-- Adding JUIZ container (name='${ARG_NAME}')")
  cmake_parse_arguments(JUIZ "" "" "SOURCES" ${ARGN})
  add_library(${ARG_NAME} SHARED ${JUIZ_SOURCES})
  target_link_libraries(${ARG_NAME} juiz_sdk)
  set_target_properties(${ARG_NAME}
      PROPERTIES
      ARCHIVE_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/>"
      LIBRARY_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/>"
      RUNTIME_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/>"
  )
endfunction()


function(add_juiz_container_process ARG_NAME)
  message("-- Adding JUIZ container-process (name='${ARG_NAME}')")
  cmake_parse_arguments(JUIZ "" "" "SOURCES" ${ARGN})
  add_library(${ARG_NAME} SHARED ${JUIZ_SOURCES})
  target_link_libraries(${ARG_NAME} juiz_sdk)
  set_target_properties(${ARG_NAME}
      PROPERTIES
      ARCHIVE_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/>"
      LIBRARY_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/>"
      RUNTIME_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/>"
  )
endfunction()


function(add_juiz_component ARG_NAME)
  message("-- Adding JUIZ component (name='${ARG_NAME}')")
  cmake_parse_arguments(JUIZ "" "" "SOURCES" ${ARGN})
  add_library(${ARG_NAME} SHARED ${JUIZ_SOURCES})
  target_link_libraries(${ARG_NAME} juiz_sdk)
  set_target_properties(${ARG_NAME}
      PROPERTIES
      ARCHIVE_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/debug>"
      LIBRARY_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/debug>"
      RUNTIME_OUTPUT_DIRECTORY "$<1:${PROJECT_HOME}/target/debug>"
  )
endfunction()

add_subdirectory(process)
add_subdirectory(container)
add_subdirectory(component)