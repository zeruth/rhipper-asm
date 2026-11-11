# The CMake project of the port of @NAME@. ras wrote this file because the project had none.
# ras does not write it again, so you can change it.
cmake_minimum_required(VERSION 3.24)
project(@TARGET@ C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)

# The translation that ras emit writes, with its runtime beside it.
set(GEN_DIR "@GEN_DIR@")
file(GLOB GEN_SOURCES CONFIGURE_DEPENDS "${GEN_DIR}/*.c" "${GEN_DIR}/*.cpp")
add_executable(@TARGET@ ${GEN_SOURCES} "${GEN_DIR}/runtime/driver.c")
target_include_directories(@TARGET@ PRIVATE "${GEN_DIR}" "${GEN_DIR}/runtime")

# The translation uses the clang extensions the runtime header relies on: 128-bit integers and
# vector types. A clang front end gets the GNU flags; clang-cl gets them through /clang:.
if(CMAKE_C_COMPILER_FRONTEND_VARIANT STREQUAL "MSVC")
  target_compile_options(@TARGET@ PRIVATE /W1 /O1 /clang:-Wno-parentheses /clang:-Wno-unused-variable /clang:-Wno-unused-but-set-variable)
else()
  target_compile_options(@TARGET@ PRIVATE -O1 -Wall -Wno-unused-variable -Wno-unused-but-set-variable -Wno-parentheses)
  if(WIN32)
    target_link_options(@TARGET@ PRIVATE -fuse-ld=lld)
  endif()
endif()
if(WIN32)
  target_link_libraries(@TARGET@ PRIVATE kernel32)
endif()
