# Toolchain-WASI-wasm32.cmake

# Target platform
set(CMAKE_SYSTEM_NAME WASI)
set(CMAKE_SYSTEM_VERSION 1)
set(CMAKE_SYSTEM_PROCESSOR wasm32)

# LLVM/Clang toolchain location
set(CLANG_BIN "/home/lind/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/bin")
set(TARGET_TRIPLE "wasm32-unknown-wasi")

# Compilers and linker
set(CMAKE_C_COMPILER   "${CLANG_BIN}/clang"   CACHE FILEPATH "" FORCE)
set(CMAKE_CXX_COMPILER "${CLANG_BIN}/clang++" CACHE FILEPATH "" FORCE)
set(CMAKE_ASM_COMPILER "${CLANG_BIN}/clang"   CACHE FILEPATH "" FORCE)
set(CMAKE_LINKER       "${CLANG_BIN}/wasm-ld" CACHE FILEPATH "" FORCE)

# Target triple
set(CMAKE_C_COMPILER_TARGET   "${TARGET_TRIPLE}" CACHE STRING "" FORCE)
set(CMAKE_CXX_COMPILER_TARGET "${TARGET_TRIPLE}" CACHE STRING "" FORCE)
set(CMAKE_ASM_COMPILER_TARGET "${TARGET_TRIPLE}" CACHE STRING "" FORCE)

# WASI sysroot
set(CMAKE_SYSROOT "/home/lind/lind-wasm/build/sysroot" CACHE PATH "" FORCE)

# Initial flags
set(CMAKE_C_FLAGS_INIT   "--target=${TARGET_TRIPLE} --sysroot=${CMAKE_SYSROOT} -fno-exceptions -fno-unwind-tables")
set(CMAKE_CXX_FLAGS_INIT "--target=${TARGET_TRIPLE} --sysroot=${CMAKE_SYSROOT} -fno-exceptions -fno-unwind-tables")
set(CMAKE_ASM_FLAGS_INIT "--target=${TARGET_TRIPLE} --sysroot=${CMAKE_SYSROOT}")
set(CMAKE_EXE_LINKER_FLAGS_INIT "--target=${TARGET_TRIPLE} --sysroot=${CMAKE_SYSROOT}")

set(CMAKE_SKIP_RPATH ON)

set(CMAKE_FIND_ROOT_PATH "${CMAKE_SYSROOT}")
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)