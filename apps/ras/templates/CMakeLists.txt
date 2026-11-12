# The CMake project of the port of @NAME@. ras wrote this file because the project had none.
# ras does not write it again, so you can change it.
cmake_minimum_required(VERSION 3.24)
project(@TARGET@ C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)

# The translation that ras emit writes, with its runtime beside it. A program is an executable;
# a DLL is a shared library whose exports (by name and ordinal, with the forwarders) are the
# image's, from the definition file ras emit writes beside the translation.
set(GEN_DIR "@GEN_DIR@")
set(RA_DLL @DLL@)
file(GLOB GEN_SOURCES CONFIGURE_DEPENDS "${GEN_DIR}/*.c" "${GEN_DIR}/*.cpp")
# The hand-written sources of the port (`sources` in rhipper.json), beside the translation: the
# functions rhipper.json names under `hand` are theirs to define, ras emit only declares them.
set(HAND_DIRS @SOURCES@)
set(HAND_SOURCES)
foreach(dir ${HAND_DIRS})
  file(GLOB_RECURSE found CONFIGURE_DEPENDS "${dir}/*.c" "${dir}/*.cpp")
  foreach(src ${found})
    cmake_path(IS_PREFIX GEN_DIR "${src}" NORMALIZE in_gen)
    if(NOT in_gen)
      list(APPEND HAND_SOURCES "${src}")
    endif()
  endforeach()
endforeach()
list(APPEND GEN_SOURCES ${HAND_SOURCES})
if(RA_DLL)
  # The definition file as a source: CMake hands it to whichever linker the toolchain has, as
  # /DEF: to link or lld-link and as an input to a MinGW ld.
  add_library(@TARGET@ SHARED ${GEN_SOURCES} "${GEN_DIR}/runtime/driver.c" "${GEN_DIR}/exports.def")
else()
  add_executable(@TARGET@ ${GEN_SOURCES} "${GEN_DIR}/runtime/driver.c")
endif()
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
  target_link_libraries(@TARGET@ PRIVATE kernel32 user32)
else()
  # The driver of a platform-neutral port binds the program's imports with dlopen.
  target_link_libraries(@TARGET@ PRIVATE ${CMAKE_DL_LIBS} m)
endif()
# The program's resources, which ras emit extracts from the image: linked into the port, so the
# program's FindResource and LoadString find them as they did.
if(EXISTS "${GEN_DIR}/resources.res")
  target_link_options(@TARGET@ PRIVATE "${GEN_DIR}/resources.res")
endif()

# A 32-bit program keeps its 32-bit pointers in the port: every address of the process must fit
# in them, which the linker arranges for a binary that is not large-address-aware.
set(RA_POINTER_BITS @POINTER_BITS@)
if(WIN32 AND RA_POINTER_BITS EQUAL 32)
  if(CMAKE_C_COMPILER_FRONTEND_VARIANT STREQUAL "MSVC")
    target_link_options(@TARGET@ PRIVATE /LARGEADDRESSAWARE:NO)
  else()
    target_link_options(@TARGET@ PRIVATE -Wl,/largeaddressaware:no)
  endif()
endif()
