# Toolchain-WASI-LLVM.cmake
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR wasm32)

# Compiler and linker
set(CLANG_ROOT "/home/lind/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04")
set(CMAKE_C_COMPILER "${CLANG_ROOT}/bin/clang")
set(CMAKE_CXX_COMPILER "${CLANG_ROOT}/bin/clang++")
set(CMAKE_LINKER "${CLANG_ROOT}/bin/wasm-ld")

# Target configuration
set(CMAKE_C_COMPILER_TARGET wasm32-unknown-wasi)
set(CMAKE_CXX_COMPILER_TARGET wasm32-unknown-wasi)

# Sysroot for WASI environment
set(CMAKE_SYSROOT "/home/lind/lind-wasm/build/sysroot")

# Force CMake to accept compilers without try-run
set(CMAKE_C_COMPILER_WORKS TRUE)
set(CMAKE_CXX_COMPILER_WORKS TRUE)
set(CMAKE_EXECUTABLE_SUFFIX ".wasm")

# Don't pass -rpath to the linker
set(CMAKE_SKIP_RPATH TRUE)
set(CMAKE_SKIP_INSTALL_RPATH TRUE)
set(CMAKE_BUILD_WITH_INSTALL_RPATH FALSE)
set(CMAKE_INSTALL_RPATH_USE_LINK_PATH FALSE)

set(CMAKE_C_FLAGS_INIT "-pthread -matomics -mbulk-memory")
set(LLVM_TEMPORARILY_ALLOW_OLD_TOOLCHAIN ON CACHE BOOL "Skip libstdc++ version check" FORCE)

set(CMAKE_CXX_STANDARD_LIBRARIES "-lc++ -lc++abi -lm" CACHE STRING "" FORCE)

set(CMAKE_CXX_FLAGS_INIT "-nostdinc++ -isystem ${LIBCXX_INCLUDE} -pthread -matomics -mbulk-memory")

set(LLVM_ENABLE_LIBCXX ON CACHE BOOL "" FORCE)

# Prevent try-run errors (because wasm can't run)
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
