# libjuiz_sdk のアーカイブの juiz_sdkConfig.cmake を確かめる小さなプロジェクト (issue #162, #273)。
# .github/workflows/sdk-release.yml が、展開したアーカイブを CMAKE_PREFIX_PATH に指定してビルドする。
#
#   cmake -S bindings/cppjuiz/tests/find_package -B build -DCMAKE_PREFIX_PATH=<アーカイブを展開したディレクトリ>
#   cmake --build build --config Release
cmake_minimum_required(VERSION 3.16)
project(juiz_sdk_find_package_test CXX)

# JUIZ_SDK_EXPECTED_VERSION を渡したときは、その版のアーカイブだけを受け付けることも確かめる
find_package(juiz_sdk ${JUIZ_SDK_EXPECTED_VERSION} EXACT CONFIG REQUIRED)
message(STATUS "juiz_sdk ${juiz_sdk_VERSION}: ${juiz_sdk_ROOT_DIR}")

add_library(find_package_process SHARED find_package_process.cpp)
target_link_libraries(find_package_process PRIVATE juiz_sdk::juiz_sdk)
# 出力先を build/out にそろえる (Visual Studio のジェネレータでも Release/ を足さないよう $<1:...> で書く)
set_target_properties(find_package_process PROPERTIES
  LIBRARY_OUTPUT_DIRECTORY "$<1:${CMAKE_BINARY_DIR}/out>"
  RUNTIME_OUTPUT_DIRECTORY "$<1:${CMAKE_BINARY_DIR}/out>")
juiz_sdk_copy_runtime(find_package_process)

# .juizapp に入れる形 (issue #287): rpath は $ORIGIN / @loader_path だけで、隣に libjuiz_sdk を置く
add_library(bundled_process SHARED find_package_process.cpp)
target_link_libraries(bundled_process PRIVATE juiz_sdk::juiz_sdk)
set_target_properties(bundled_process PROPERTIES
  LIBRARY_OUTPUT_DIRECTORY "$<1:${CMAKE_BINARY_DIR}/bundle>"
  RUNTIME_OUTPUT_DIRECTORY "$<1:${CMAKE_BINARY_DIR}/bundle>")
juiz_sdk_bundle(bundled_process)
