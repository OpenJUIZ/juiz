cmake_minimum_required(VERSION 3.13)

project(juiz_cpp_component CXX)


add_subdirectory(example_component)
add_subdirectory(cpp_typed_component)
add_subdirectory(cpp_typed_component_other_abi)
add_subdirectory(cpp_image_component)

# OpenCV があるときだけ (juiz/opencv.h の例)
find_package(OpenCV QUIET COMPONENTS core)
if(OpenCV_FOUND)
  add_subdirectory(cpp_image_opencv_component)
else()
  message("-- OpenCV が見つからないので cpp_image_opencv_component をビルドしません")
endif()
