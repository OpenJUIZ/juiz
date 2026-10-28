cmake_minimum_required(VERSION 3.13)
project(cpp_image_opencv_component CXX)

# juiz/opencv.h (juiz::Image と cv::Mat の変換) のテスト用 (juiz_core/tests/integration/plugin_image_test.rs、issue #253)。
# OpenCV の core だけを使う。examples/cpp/component/CMakeLists.txt が OpenCV を見つけたときだけビルドする
find_package(OpenCV REQUIRED COMPONENTS core)
add_juiz_component(${PROJECT_NAME} SOURCES ${PROJECT_NAME}.cpp)
target_include_directories(${PROJECT_NAME} PRIVATE ${OpenCV_INCLUDE_DIRS})
target_link_libraries(${PROJECT_NAME} ${OpenCV_LIBS})
