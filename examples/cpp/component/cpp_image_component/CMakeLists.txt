cmake_minimum_required(VERSION 3.13)
project(cpp_image_component CXX)

# 画像の引数・戻り値のテスト用 (juiz_core/tests/integration/cpp_image_test.rs、issue #253)
add_juiz_component(${PROJECT_NAME} SOURCES ${PROJECT_NAME}.cpp)
