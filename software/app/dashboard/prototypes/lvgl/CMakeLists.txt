cmake_minimum_required(VERSION 3.16)
project(kvmpanel_proto_lvgl C)

set(CMAKE_C_STANDARD 11)
if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

# LVGL lives outside the repo (see sources/README.md).
set(LVGL_DIR "$ENV{HOME}/work/KVMPanel/upstream/lvgl" CACHE PATH "LVGL source tree")

set(LV_BUILD_CONF_PATH "${CMAKE_CURRENT_SOURCE_DIR}/lv_conf.h" CACHE PATH "" FORCE)
set(CONFIG_LV_BUILD_DEMOS OFF CACHE BOOL "" FORCE)
set(CONFIG_LV_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(CONFIG_LV_USE_THORVG OFF CACHE BOOL "" FORCE)
set(CONFIG_LV_USE_THORVG_INTERNAL OFF CACHE BOOL "" FORCE)
set(LV_BUILD_INSTALL OFF CACHE BOOL "" FORCE)
add_subdirectory(${LVGL_DIR} lvgl)

find_package(X11 REQUIRED)

add_executable(proto_lvgl
  main.c
  x11_out.c
  fonts.c
  bg.c
  spark.c
  ui.c
  panel_sock.c
  third_party/cjson/cJSON.c
)
target_include_directories(proto_lvgl PRIVATE ${CMAKE_CURRENT_SOURCE_DIR} third_party/cjson)
target_link_libraries(proto_lvgl PRIVATE lvgl ${X11_LIBRARIES} m)
