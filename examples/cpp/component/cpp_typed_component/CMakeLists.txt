cmake_minimum_required(VERSION 3.13)
project(cpp_typed_component CXX)

# 型付きの引数・戻り値とネイティブ値のテスト用 (juiz_core/tests/cpp_typed_test.rs、issue #195・#177)。
# 型は juiz_idl のテストと同じ IDL から作る
set(IDL_DIR ${PROJECT_HOME}/juiz_idl/test_package/idl)
add_juiz_component(${PROJECT_NAME} SOURCES ${PROJECT_NAME}.cpp)
juiz_generate_types(${PROJECT_NAME}
  IDL ${IDL_DIR}/geometry.jidl
  DEPENDS ${IDL_DIR}/common.jidl ${IDL_DIR}/stamp.avsc)
