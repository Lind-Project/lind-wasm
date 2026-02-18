# Toolchain for building libc++/libc++abi and compiler-rt for wasm32-unknown-wasi
# against the Lind glibc sysroot (issue #245).
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR wasm32)

# CLANG_BIN: directory containing clang, clang++, wasm-ld (e.g. from apt.llvm.org or tarball)
if(NOT DEFINED CLANG_BIN)
  set(CLANG_BIN "/usr/bin")
endif()
set(CMAKE_C_COMPILER "${CLANG_BIN}/clang")
set(CMAKE_CXX_COMPILER "${CLANG_BIN}/clang++")
set(CMAKE_LINKER "${CLANG_BIN}/wasm-ld")

# Lind sysroot (glibc + C headers); default = repo root when toolchain is at repo root
if(NOT DEFINED CMAKE_SYSROOT)
  get_filename_component(_REPO_ROOT "${CMAKE_CURRENT_LIST_DIR}" ABSOLUTE)
  set(CMAKE_SYSROOT "${_REPO_ROOT}/src/glibc/sysroot")
endif()

set(CMAKE_C_COMPILER_TARGET wasm32-unknown-wasi)
set(CMAKE_CXX_COMPILER_TARGET wasm32-unknown-wasi)

# -ftls-model=local-exec required for wasm32 (LLVM only supports it for non-Emscripten)
set(CMAKE_C_FLAGS_INIT "-pthread -matomics -mbulk-memory -ftls-model=local-exec -static -nostdlib -nodefaultlibs -fno-exceptions -fno-unwind-tables")
set(CMAKE_CXX_FLAGS_INIT "-frtti -pthread -matomics -mbulk-memory -ftls-model=local-exec -static -nostdlib -nodefaultlibs -fno-exceptions -stdlib=libc++ -fno-unwind-tables")
set(CMAKE_EXE_LINKER_FLAGS_INIT "-static -nostdlib -nodefaultlibs")

set(CMAKE_SKIP_RPATH ON)
set(LLVM_HOST_TRIPLE "wasm32-wasip1")
set(LLVM_DEFAULT_TARGET_TRIPLE "x86_64-unknown-linux-gnu")

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
