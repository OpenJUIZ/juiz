# プロセスを宣言するときの操作を関数にしておく。
# まとめて変更できるように。
# コンテナやコンポーネントも全く同じ操作だけど、一応、別にしておく。
function(add_juiz_process ARG_NAME)
  message("-- Adding JUIZ process (name='${ARG_NAME}')")
  cmake_parse_arguments(JUIZ "" "" "SOURCES" ${ARGN})
  add_library(${ARG_NAME} SHARED ${JUIZ_SOURCES})
  target_link_libraries(${ARG_NAME} juiz_sdk)
  # set_target_properties(${ARG_NAME}
  #     PROPERTIES
  #     ARCHIVE_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/"
  #     LIBRARY_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/"
  #     RUNTIME_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/"
  # )
endfunction()


function(add_juiz_container ARG_NAME)
  message("-- Adding JUIZ container (name='${ARG_NAME}')")
  cmake_parse_arguments(JUIZ "" "" "SOURCES" ${ARGN})
  add_library(${ARG_NAME} SHARED ${JUIZ_SOURCES})
  target_link_libraries(${ARG_NAME} juiz_sdk)
  set_target_properties(${ARG_NAME}
      PROPERTIES
      ARCHIVE_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/"
      LIBRARY_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/"
      RUNTIME_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/"
  )
endfunction()


function(add_juiz_container_process ARG_NAME)
  message("-- Adding JUIZ container-process (name='${ARG_NAME}')")
  cmake_parse_arguments(JUIZ "" "" "SOURCES" ${ARGN})
  add_library(${ARG_NAME} SHARED ${JUIZ_SOURCES})
  target_link_libraries(${ARG_NAME} juiz_sdk)
  set_target_properties(${ARG_NAME}
      PROPERTIES
      ARCHIVE_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/"
      LIBRARY_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/"
      RUNTIME_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/"
  )
endfunction()


function(add_juiz_component ARG_NAME)
  message("-- Adding JUIZ component (name='${ARG_NAME}')")
  cmake_parse_arguments(JUIZ "" "" "SOURCES" ${ARGN})
  add_library(${ARG_NAME} SHARED ${JUIZ_SOURCES})
  target_link_libraries(${ARG_NAME} juiz_sdk)
  set_target_properties(${ARG_NAME}
      PROPERTIES
      ARCHIVE_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/debug"
      LIBRARY_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/debug"
      RUNTIME_OUTPUT_DIRECTORY "${PROJECT_HOME}/target/debug"
  )
endfunction()
