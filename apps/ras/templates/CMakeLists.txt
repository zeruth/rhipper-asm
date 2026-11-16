# The CMake project of the port of @NAME@. ras wrote this file because the project had none.
# ras does not write it again, so you can change it.
cmake_minimum_required(VERSION 3.24)
project(@TARGET@ C CXX)

# C23: the large data sections are `#embed`s of their bytes, beside the translation.
set(CMAKE_C_STANDARD 23)
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
  # A C++ port calls msvcp140 under its decorated names, and the static part of its import
  # library besides (`std::_Facet_Register`, the `__std_*` vector algorithms), which no definition
  # file can give: msvcprt.lib links both, as the oracle's build does.
  set(RA_CPP_SOURCES ${GEN_SOURCES})
  list(FILTER RA_CPP_SOURCES INCLUDE REGEX "\\.cpp$")
  if(RA_CPP_SOURCES)
    target_link_libraries(@TARGET@ PRIVATE msvcprt)
  endif()
  # The import library of each DLL the program imports, made from the definition file ras emit
  # wrote from the image's import table (`imports/<dll>.def`), so no SDK needs to have one: an
  # MSVC-style toolchain's librarian takes /def, a GNU-style clang's llvm-dlltool does the same.
  file(GLOB RA_IMPORT_DEFS CONFIGURE_DEPENDS "${GEN_DIR}/imports/*.def")
  if(RA_IMPORT_DEFS)
    if(CMAKE_SYSTEM_PROCESSOR MATCHES "ARM64|aarch64")
      set(RA_LIB_MACHINE ARM64)
      set(RA_DLLTOOL_MACHINE arm64)
    else()
      set(RA_LIB_MACHINE X64)
      set(RA_DLLTOOL_MACHINE i386:x86-64)
    endif()
    if(NOT CMAKE_C_COMPILER_FRONTEND_VARIANT STREQUAL "MSVC")
      get_filename_component(RA_LLVM_BIN "${CMAKE_C_COMPILER}" DIRECTORY)
      find_program(RA_DLLTOOL llvm-dlltool HINTS "${RA_LLVM_BIN}" REQUIRED)
    endif()
    set(RA_IMPORT_LIBS)
    foreach(def ${RA_IMPORT_DEFS})
      get_filename_component(stem "${def}" NAME_WE)
      set(lib "${CMAKE_BINARY_DIR}/imports/${stem}.lib")
      if(CMAKE_C_COMPILER_FRONTEND_VARIANT STREQUAL "MSVC")
        set(make_lib "${CMAKE_AR}" /nologo "/def:${def}" "/out:${lib}" "/machine:${RA_LIB_MACHINE}")
      else()
        set(make_lib "${RA_DLLTOOL}" -m ${RA_DLLTOOL_MACHINE} -d "${def}" -l "${lib}")
      endif()
      add_custom_command(OUTPUT "${lib}" COMMAND ${CMAKE_COMMAND} -E make_directory "${CMAKE_BINARY_DIR}/imports"
        COMMAND ${make_lib} DEPENDS "${def}" VERBATIM)
      list(APPEND RA_IMPORT_LIBS "${lib}")
    endforeach()
    add_custom_target(@TARGET@_imports DEPENDS ${RA_IMPORT_LIBS})
    add_dependencies(@TARGET@ @TARGET@_imports)
    target_link_libraries(@TARGET@ PRIVATE ${RA_IMPORT_LIBS})
  endif()
else()
  # The driver of a platform-neutral port binds the program's imports with dlopen; a Windows
  # program's kernel32, user32 and C runtime imports are the driver's Win32 layer over the host's
  # libraries, which dlsym finds in the executable's own exports. `ras doctor --host <exe>` names
  # the imports the layer lacks.
  find_package(Threads REQUIRED)
  set_target_properties(@TARGET@ PROPERTIES ENABLE_EXPORTS ON)
  target_link_libraries(@TARGET@ PRIVATE Threads::Threads ${CMAKE_DL_LIBS} m)
endif()
# The program's resources, which ras emit extracts from the image: linked into the port, so the
# program's FindResource and LoadString find them as they did.
if(EXISTS "${GEN_DIR}/resources.res")
  target_link_options(@TARGET@ PRIVATE "${GEN_DIR}/resources.res")
endif()

# The residual engine (`build.residual` in rhipper.json), the static library of ra-asm-residual:
# a call of the port into code of the image that no lifted function has runs there, over the
# reference's bytes, which the driver maps at the image's base. At exit the port prints what the
# engine entered and lists it in build/residual.json. The library is Rust's, built against the
# DLL C runtime, which the port then takes as well; its system libraries are those of Rust's std.
set(RA_RESIDUAL "@RESIDUAL@")
if(WIN32 AND RA_RESIDUAL)
  target_compile_definitions(@TARGET@ PRIVATE RA_RESIDUAL=1 "RA_RESIDUAL_IMAGE=\"@EXE@\""
    "RA_RESIDUAL_FACTS=\"${CMAKE_CURRENT_SOURCE_DIR}/build/residual.json\"")
  set_property(TARGET @TARGET@ PROPERTY MSVC_RUNTIME_LIBRARY "MultiThreadedDLL")
  target_link_libraries(@TARGET@ PRIVATE "${RA_RESIDUAL}" advapi32 ntdll userenv ws2_32 dbghelp bcrypt synchronization)
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
elseif(RA_POINTER_BITS EQUAL 32)
  # On x86-64 Linux the executable is linked at its fixed low address, and the driver keeps the
  # stacks and the heap below 2 GB (MAP_32BIT and the brk heap).
  set_target_properties(@TARGET@ PROPERTIES POSITION_INDEPENDENT_CODE OFF)
  target_compile_options(@TARGET@ PRIVATE -fno-pie)
  target_link_options(@TARGET@ PRIVATE -no-pie)
endif()
