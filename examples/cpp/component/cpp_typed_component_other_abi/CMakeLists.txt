cmake_minimum_required(VERSION 3.13)
project(cpp_typed_component_other_abi CXX)

# cpp_typed_component と同じ型を、ABI タグを変えて受け取るテスト用 (juiz_core/tests/cpp_typed_test.rs)。
# ABI タグが違うので、cpp_typed_component の値は辞書を通して届く
set(IDL_DIR ${PROJECT_HOME}/juiz_idl/test_package/idl)
add_juiz_component(${PROJECT_NAME} SOURCES ${PROJECT_NAME}.cpp)
target_compile_definitions(${PROJECT_NAME} PRIVATE JUIZ_CPP_ABI_TAG_EXTRA="other")
juiz_generate_types(${PROJECT_NAME}
  IDL ${IDL_DIR}/geometry.jidl
  DEPENDS ${IDL_DIR}/common.jidl ${IDL_DIR}/stamp.avsc)
