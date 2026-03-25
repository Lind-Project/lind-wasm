#!/bin/bash
set -e

export HOME_DIR="/home/lind/lind-wasm"
export LLVM_SRC="$HOME_DIR/llvm-project/llvm"
export BUILD_DIR="$HOME_DIR/llvm-wasm-build"
export TOOLCHAIN_FILE="$HOME_DIR/Toolchain-WASI-LLVM.cmake"
export LIBCXX_INCLUDE="$HOME_DIR/build/sysroot/include/wasm32-wasi/c++/v1/"
export NATIVE_TOOL_DIR="$HOME_DIR/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/bin/"
export FIX_HEADER="$HOME_DIR/build/sysroot/include/wasm32-wasi/c++/v1/__algorithm/fix_std_maxmin.h"

mkdir -p "$BUILD_DIR"

cmake -B "$BUILD_DIR" -S "$LLVM_SRC" \
  -DCMAKE_TOOLCHAIN_FILE="$TOOLCHAIN_FILE" \
  -DCMAKE_BUILD_TYPE=Release \
  -DLLVM_ENABLE_PROJECTS="clang;lld" \
  -DLLVM_HOST_TRIPLE="wasm32-unknown-wasi" \
  -DLLVM_DEFAULT_TARGET_TRIPLE="x86_64-unknown-linux-gnu" \
  -DLLVM_TARGETS_TO_BUILD="X86" \
  -DLLD_ENABLE_TARGETS="ELF" \
  -DLLVM_TOOL_LLD_BUILD=ON \
  -DCMAKE_C_FLAGS="-fno-exceptions -fno-unwind-tables -L/usr/lib/gcc/x86_64-linux-gnu/13" \
  -DCMAKE_CXX_FLAGS="-include $FIX_HEADER -fno-exceptions \
      -Wno-error=template-argument-type-deduction \
      -fno-unwind-tables -fno-rtti -I$LIBCXX_INCLUDE -D__GNU__ -D_POSIX_C_SOURCE=200809L -L/usr/lib/gcc/x86_64-linux-gnu/13" \
  -DCMAKE_EXE_LINKER_FLAGS=" \
  	-L$HOME_DIR/build/sysroot/lib/wasm32-wasi \
  	-Wl,--export=__stack_pointer,--export=__stack_low \
  	-Wl,--import-memory,--export-memory \
  	-Wl,--max-memory=67108864 \
    -Wl,--no-entry \
  	-lm " \
  -DLLVM_TOOL_LLI_BUILD=OFF \
  -DLLVM_TOOL_LLVM_JITLINK_EXECUTOR_BUILD=OFF \
  -DCMAKE_C_STANDARD_LIBRARIES="-lc -lcompiler_rt" \
  -DCMAKE_CXX_STANDARD_LIBRARIES="-lc++ -lc++abi -lcompiler_rt -lc" \
  -DCMAKE_INSTALL_RPATH="$BUILD_DIR" \
  -DCMAKE_INSTALL_PREFIX="$BUILD_DIR" \
  -DCMAKE_SKIP_RPATH=ON \
  -DCMAKE_SKIP_INSTALL_RPATH=ON \
  -DLLVM_ENABLE_THREADS=OFF \
  -DLLVM_BUILD_TESTS=OFF \
  -DHAVE_CXX_ATOMICS_WITHOUT_LIB=1 \
  -DHAVE_CXX_ATOMICS_WITH_LIB=0 \
  -DHAVE_CXX_ATOMICS64_WITHOUT_LIB=1 \
  -DHAVE_CXX_ATOMICS64_WITH_LIB=0 \
  -DHAVE_LIBATOMIC=0 \
  -DHAVE_LIBRT=0 \
  -DLLVM_INCLUDE_GOOGLETEST=OFF \
  -DLLVM_TOOL_CLANG_BUILD=ON \
  -DCLANG_INCLUDE_TESTS=OFF \
  -DLLD_INCLUDE_TESTS=OFF \
  -DLLVM_ENABLE_PIC=OFF \
  -DLLVM_BUILD_SHARED_LIBS=OFF \
  -DLLVM_BUILD_TOOLS=OFF \
  -DLLVM_INCLUDE_TESTS=OFF \
  -DLLVM_INCLUDE_EXAMPLES=OFF \
  -DLLVM_ENABLE_LIBCXX=ON \
  -DLLVM_NATIVE_TOOL_DIR="$NATIVE_TOOL_DIR" \
  -DLLVM_INCLUDE_BENCHMARKS=OFF

# cmake --build "$BUILD_DIR" --target clang
# cmake --build "$BUILD_DIR" --target lld
cmake --build "$BUILD_DIR" --target install-core-resource-headers

# -DCMAKE_SHARED_LINKER_FLAGS="\
  #   -L/usr/lib/gcc/x86_64-linux-gnu/13" \
  # -DCMAKE_REQUIRED_LIBRARIES=atomic \