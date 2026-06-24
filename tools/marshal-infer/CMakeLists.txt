cmake_minimum_required(VERSION 3.13.4)
project(marshal-infer CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Drive everything from `llvm-config` rather than find_package(LLVM): the shipped
# clang+llvm-18.1.8 tree exports CMake targets with unresolved transitive deps
# (ZLIB::ZLIB / Terminfo::terminfo), so flat llvm-config flags are more robust.
# Override the binary with -DLLVM_CONFIG=/path/to/llvm-config.
find_program(LLVM_CONFIG llvm-config REQUIRED)
message(STATUS "marshal-infer: using ${LLVM_CONFIG}")

execute_process(COMMAND ${LLVM_CONFIG} --includedir
  OUTPUT_VARIABLE LLVM_INCLUDE_DIR OUTPUT_STRIP_TRAILING_WHITESPACE)
execute_process(COMMAND ${LLVM_CONFIG} --link-static --libs core irreader support
  OUTPUT_VARIABLE LLVM_LIBS OUTPUT_STRIP_TRAILING_WHITESPACE)
execute_process(COMMAND ${LLVM_CONFIG} --link-static --system-libs
  OUTPUT_VARIABLE LLVM_SYSTEM_LIBS OUTPUT_STRIP_TRAILING_WHITESPACE)
execute_process(COMMAND ${LLVM_CONFIG} --libdir
  OUTPUT_VARIABLE LLVM_LIBDIR OUTPUT_STRIP_TRAILING_WHITESPACE)

separate_arguments(LLVM_LIBS)
separate_arguments(LLVM_SYSTEM_LIBS)

# This box ships only runtime sonames for zlib/terminfo (libz.so.1, libtinfo.so.6)
# with no -dev symlinks, so plain `-lz`/`-ltinfo` fail to link. Resolve those two
# to their versioned full paths and drop the bare -l flags.
set(_resolved_syslibs "")
foreach(_lib IN LISTS LLVM_SYSTEM_LIBS)
  if(_lib STREQUAL "-lz")
    find_library(ZLIB_FULL NAMES z libz.so.1 PATHS /usr/lib/x86_64-linux-gnu /lib/x86_64-linux-gnu)
    list(APPEND _resolved_syslibs ${ZLIB_FULL})
  elseif(_lib STREQUAL "-ltinfo")
    find_library(TINFO_FULL NAMES tinfo libtinfo.so.6 PATHS /usr/lib/x86_64-linux-gnu /lib/x86_64-linux-gnu)
    list(APPEND _resolved_syslibs ${TINFO_FULL})
  else()
    list(APPEND _resolved_syslibs ${_lib})
  endif()
endforeach()
set(LLVM_SYSTEM_LIBS ${_resolved_syslibs})

add_executable(marshal-infer
  src/main.cpp
  src/ParamTree.cpp
  src/Infer.cpp
  src/Annotations.cpp
)
target_include_directories(marshal-infer PRIVATE
  ${LLVM_INCLUDE_DIR}
  ${CMAKE_CURRENT_SOURCE_DIR}/src)
# LLVM's static libs are built without RTTI; match that to avoid link errors.
target_compile_options(marshal-infer PRIVATE -fno-rtti)
target_link_directories(marshal-infer PRIVATE ${LLVM_LIBDIR})
target_link_libraries(marshal-infer PRIVATE ${LLVM_LIBS} ${LLVM_SYSTEM_LIBS})
