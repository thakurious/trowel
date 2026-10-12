cmake_minimum_required(VERSION 3.16)
project(trowel-worker CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

option(TROWEL_VULKAN "Link llama.cpp with GGML_VULKAN" ON)
option(TROWEL_BUILD_WORKER "Build trowel-worker (needs cpp/llama/src)" ON)

add_library(trowel_image_sizing STATIC image_sizing.cpp)
target_include_directories(trowel_image_sizing PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})

add_executable(trowel-image-sizing-test image_sizing_test.cpp)
target_link_libraries(trowel-image-sizing-test PRIVATE trowel_image_sizing)

if(NOT TROWEL_BUILD_WORKER)
  return()
endif()

set(LLAMA_SRC "${CMAKE_CURRENT_SOURCE_DIR}/llama/src")
if(NOT EXISTS "${LLAMA_SRC}/CMakeLists.txt")
  message(FATAL_ERROR "llama.cpp missing at ${LLAMA_SRC}; run scripts/build-llama.sh")
endif()

set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
set(GGML_VULKAN ${TROWEL_VULKAN} CACHE BOOL "" FORCE)
set(LLAMA_BUILD_MTMD ON CACHE BOOL "" FORCE)
set(LLAMA_BUILD_COMMON OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_TOOLS OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_SERVER OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_APP OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_UI OFF CACHE BOOL "" FORCE)
set(LLAMA_OPENSSL OFF CACHE BOOL "" FORCE)

add_subdirectory("${LLAMA_SRC}" llama-build EXCLUDE_FROM_ALL)

add_executable(trowel-worker main.cpp embedder.cpp)
target_include_directories(trowel-worker PRIVATE
  "${LLAMA_SRC}/include"
  "${LLAMA_SRC}/ggml/include"
  "${LLAMA_SRC}/tools/mtmd"
  "${LLAMA_SRC}/vendor")
target_link_libraries(trowel-worker PRIVATE llama mtmd trowel_image_sizing)
if(TROWEL_VULKAN)
  target_compile_definitions(trowel-worker PRIVATE TROWEL_VULKAN=1)
endif()
