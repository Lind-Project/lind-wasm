# Toolchain-WASI-wasm32.cmake

# Target platform ("WASI")
set(CMAKE_SYSTEM_NAME WASI) # system name 
set(CMAKE_SYSTEM_VERSION 1) # not necessary?
set(CMAKE_SYSTEM_PROCESSOR wasm32)  #taken from original cmake

# LLVM/Clang toolchain location, hard code is fine, note version changes
# set(CLANG_BIN "/home/lind/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin")
set(CLANG_BIN "/home/lind/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/bin")   #ver updated

# Compilers and linker (chatGPT suggest cache override, not sure if necessary)
# set(CMAKE_C_COMPILER "${CLANG_BIN}/clang")
set(CMAKE_C_COMPILER   "${CLANG_BIN}/clang"   CACHE FILEPATH "" FORCE)

# set(CMAKE_CXX_COMPILER "${CLANG_BIN}/clang++")
set(CMAKE_CXX_COMPILER "${CLANG_BIN}/clang++" CACHE FILEPATH "" FORCE)

# this is entirely new
set(CMAKE_ASM_COMPILER "${CLANG_BIN}/clang"   CACHE FILEPATH "" FORCE)

# set(CMAKE_LINKER "${CLANG_BIN}/bin/wasm-ld")
set(CMAKE_LINKER       "${CLANG_BIN}/wasm-ld" CACHE FILEPATH "" FORCE)

# Target triple

set(TARGET_TRIPLE "wasm32-unknown-wasi")    #same as host triple

set(CMAKE_C_COMPILER_TARGET   "${TARGET_TRIPLE}" CACHE STRING "" FORCE)
set(CMAKE_CXX_COMPILER_TARGET "${TARGET_TRIPLE}" CACHE STRING "" FORCE)
set(CMAKE_ASM_COMPILER_TARGET "${TARGET_TRIPLE}" CACHE STRING "" FORCE)

# WASI sysroot, note path updated
# set(CMAKE_SYSROOT "/home/lind/lind-wasm/src/glibc/sysroot")
set(CMAKE_SYSROOT "/home/lind/lind-wasm/build/sysroot" CACHE PATH "" FORCE)

# Initial flags, unwindlib ignored, as previously clang++ 
set(CMAKE_C_FLAGS_INIT   "--target=${TARGET_TRIPLE} --sysroot=${CMAKE_SYSROOT} -fno-exceptions -fno-unwind-tables")
set(CMAKE_CXX_FLAGS_INIT "--target=${TARGET_TRIPLE} --sysroot=${CMAKE_SYSROOT} -fno-exceptions -fno-unwind-tables")
set(CMAKE_ASM_FLAGS_INIT "--target=${TARGET_TRIPLE} --sysroot=${CMAKE_SYSROOT}")
set(CMAKE_EXE_LINKER_FLAGS_INIT "--target=${TARGET_TRIPLE} --sysroot=${CMAKE_SYSROOT}")

set(CMAKE_SKIP_RPATH ON)

set(CMAKE_FIND_ROOT_PATH "${CMAKE_SYSROOT}")

# three settings below as they were, no changes
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)

# this is new
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)