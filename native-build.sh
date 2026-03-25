export HOME_DIR=/home/lind/lind-wasm
export LLVM_SRC=$HOME_DIR/llvm-project/llvm
export NATIVE_BUILD=$HOME_DIR/llvm-native-build

cmake -B "$NATIVE_BUILD" -S "$LLVM_SRC" \
  -DCMAKE_BUILD_TYPE=Release \
  -DLLVM_ENABLE_PROJECTS="clang;lld" \
  -DLLVM_INCLUDE_TESTS=OFF \
  -DLLVM_BUILD_TESTS=OFF \
  -DLLVM_INCLUDE_BENCHMARKS=OFF \
  -DLLVM_INCLUDE_EXAMPLES=OFF
  