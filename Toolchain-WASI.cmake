# Toolchain-WASI.cmake
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR wasm32)

set(CLANG_BIN "/home/lind/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/bin")
set(CMAKE_C_COMPILER "${CLANG_BIN}/clang")
set(CMAKE_CXX_COMPILER "${CLANG_BIN}/clang++")
set(CMAKE_LINKER "${CLANG_BIN}/bin/wasm-ld")
set(CMAKE_SYSROOT "/home/lind/lind-wasm/build/sysroot")

set(CMAKE_C_COMPILER_TARGET wasm32-unknown-wasi)
set(CMAKE_CXX_COMPILER_TARGET wasm32-unknown-wasi)

set(CMAKE_C_FLAGS_INIT "-pthread -matomics -mbulk-memory -static -nostdlib -nodefaultlibs -fno-exceptions -fno-unwind-tables")
set(CMAKE_CXX_FLAGS_INIT "-frtti -pthread -matomics -mbulk-memory -static -nostdlib -nodefaultlibs -fno-exceptions -stdlib=libc++ -fno-unwind-tables")
set(CMAKE_EXE_LINKER_FLAGS_INIT "-static -nostdlib -nodefaultlibs")

# Optional: disable rpath injection
set(CMAKE_SKIP_RPATH ON)

# These fix platform error
set(LLVM_HOST_TRIPLE "wasm32-wasip1")
set(LLVM_DEFAULT_TARGET_TRIPLE "wasm32-wasip1")

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
