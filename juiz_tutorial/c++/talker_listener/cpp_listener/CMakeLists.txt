cmake_minimum_required(VERSION 3.13)
project(cpp_listener CXX)


add_juiz_process(${PROJECT_NAME} SOURCES ${PROJECT_NAME}.cpp)

# link_directories(${PROJECT_HOME}/target/debug)
# add_library(${PROJECT_NAME} SHARED  ${PROJECT_NAME}.cpp)
# target_link_libraries(${ARG_NAME} juiz_sdk)