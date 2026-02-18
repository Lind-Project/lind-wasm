# C++ sysroot per [issue #245](https://github.com/Lind-Project/lind-wasm/issues/245)

This doc describes the integration of the [issue #245](https://github.com/Lind-Project/lind-wasm/issues/245) approach so that `repro-cpp-error/hello.cpp` can **compile** and **link** with the Lind wasm32-unknown-wasi sysroot.

## Applied changes (issue #245)

### 1. Glibc

- **`src/glibc/target/include/bits/struct_stat.h`**  
  `struct stat` uses explicit time fields: `st_atime`, `st_atimensec`, `st_mtime`, `st_mtimensec`, `st_ctime`, `st_ctimensec` (no `struct timespec` members).

- **`src/glibc/bits/statvfs.h`**  
  First `struct statvfs` has `unsigned int __f_type` for libc++ `<filesystem>`.

### 2. Libc++ (in `llvm-project/`)

- **`libcxx/src/filesystem/filesystem_common.h`**  
  - `convert_to_timespec()`: use `reinterpret_cast<long*>(&dest.tv_sec)` / `tv_nsec` in `set_times_checked()`.
  - For `__wasi__`/`__wasm32__`: `extract_atime`/`extract_mtime` use `st_atime`/`st_atimensec` and `st_mtime`/`st_mtimensec` to build a `TimeSpec`.

- **`libcxx/include/__support/musl/xlocale.h`**  
  Last parameter of the `*_l` functions remains `locale_t`; `wcstoull_l` return type is `unsigned long long`.

### 3. Shim header and libs

- **`scripts/shim-headers/__algorithm/fix_std_maxmin.h`**  
  `std::max`/`std::min` overloads for two arguments; installed under sysroot `include/c++/v1/__algorithm/` by `build_libcxx_wasi.sh`.

- **`scripts/shim-libs/`**  
  `fenv_shim.c`, `eh_stub.c`, `lll_elision_shim.c` → built by `scripts/build_sysroot_shims.sh` and installed as `libfenv_shim.a`, `libeh_stub.a`, `lll_shim.a` in `sysroot/lib/wasm32-wasi/`.

### 4. Toolchain and scripts

- **`Toolchain-WASI.cmake`**  
  WASI toolchain for building libc++/libc++abi (and optionally compiler-rt) against the Lind sysroot; includes `-ftls-model=local-exec`.

- **Scripts**  
  - `scripts/prepare_sysroot_from_glibc.sh` – copy `target/include` and `target/lib` into sysroot.
  - `scripts/build_sysroot_shims.sh` – build and install shim libs.
  - `scripts/build_libcxx_wasi.sh` – build libc++/libc++abi, install into sysroot, install `fix_std_maxmin.h`.

## Build order

1. **Glibc sysroot**  
   Run `./scripts/make_glibc_and_sysroot.sh` (or otherwise ensure `src/glibc/target/include` and `src/glibc/target/lib` exist and are copied into the sysroot).

2. **Prepare sysroot**  
   `./scripts/prepare_sysroot_from_glibc.sh`

3. **Shim libs**  
   `./scripts/build_sysroot_shims.sh`

4. **Libc++**  
   Requires `llvm-project` at repo root (e.g. `git clone --branch llvmorg-16.0.4 https://github.com/llvm/llvm-project.git`).  
   Then: `./scripts/build_libcxx_wasi.sh`

5. **Compiler-rt (optional, for full link)**  
   Build compiler-rt for wasm32 per issue #245 and copy `libclang_rt.builtins-wasm32.a` into `sysroot/lib/wasm32-wasi/` (e.g. as `libcompiler_rt.a` or the path expected by the driver).

## Verify

**Compile only** (no linker):

```bash
clang++ --target=wasm32-unknown-wasi --sysroot="$PWD/src/glibc/sysroot" \
  -c repro-cpp-error/hello.cpp -o hello.o
```

**Full link** (needs `libc.a` in sysroot and wasm-ld; compiler-rt required by default):

```bash
clang++ --target=wasm32-unknown-wasi --sysroot="$PWD/src/glibc/sysroot" \
  repro-cpp-error/hello.cpp -o hello.wasm
```

If the sysroot does not contain `libc.a` (from a successful glibc build), link will fail with “unable to find library -lc”. If compiler-rt is missing, link will fail looking for `libclang_rt.builtins-wasm32.a`. Resolve by completing the glibc build and/or building and installing compiler-rt as in issue #245.
