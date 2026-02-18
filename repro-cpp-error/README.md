# Reproduce: C++ headers missing in wasm32-unknown-wasi sysroot

This directory reproduces the error when compiling C++ code against the Lind sysroot, which currently only provides C headers (no libc++).

## Prerequisites

- `clang++` on PATH (with wasm32-unknown-wasi target support)
- A minimal sysroot at `src/glibc/sysroot` (created by the main repo script or the minimal layout used for this repro)

## Reproduce the error

From the **repository root**:

```bash
# Compile only (fails at first C++ header)
clang++ --target=wasm32-unknown-wasi --sysroot="$PWD/src/glibc/sysroot" \
  -c repro-cpp-error/hello.cpp -o hello.o

# Or compile + link (same failure)
clang++ --target=wasm32-unknown-wasi --sysroot="$PWD/src/glibc/sysroot" \
  -Wl,--import-memory,--export-memory \
  repro-cpp-error/hello.cpp -o hello.wasm
```

## Expected output

```
repro-cpp-error/hello.cpp:1:10: fatal error: 'algorithm' file not found
#include <algorithm>
         ^~~~~~~~~~~
1 error generated.
```

The sysroot at `src/glibc/sysroot` has no `include/c++/v1/` (libc++ headers) or `lib/wasm32-wasi/libc++.a`, so C++ standard library includes fail.
