cmake_minimum_required(VERSION 3.13)
project(talker_listener_cpp CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# juiz のヘッダは BOM なしの UTF-8 で日本語のコメントを含むので、MSVC にも UTF-8 として読ませる
if (MSVC)
  add_compile_options(/utf-8)
endif ()

set(JUIZ_HEADER_DIR bindings/cppjuiz/include)
set(PROJECT_HOME ${PROJECT_SOURCE_DIR}/../../..)
include_directories(${PROJECT_HOME}/${JUIZ_HEADER_DIR})
link_directories(${PROJECT_HOME}/target/debug)

include(../juiz.mk)

add_subdirectory(cpp_listener)
add_subdirectorY(cpp_talker)

