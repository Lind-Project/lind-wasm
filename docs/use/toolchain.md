# Lind toolchain

The toolchain to build and run lind programs consists of the following components:

- *Clang* with *WebAssembly System Interface* (WASI) support to build *glibc*
  and lind programs
- A custom [*glibc*](../internal/libc.md) used as sysroot to build
  lind programs
- A custom [`wasm-opt`](../internal/multiprocess-support.md) binary to enable multi-processing
  in lind programs
- A custom *WebAssembly* runtime ([`wasmtime`](../internal/wasmtime.md)) with
  [*RawPOSIX*](../internal/rawposix.md) to run lind programs
- *Cargo* to build `wasmtime`

This document gives an overview of how the toolchain is built. The build process
is automated with *Docker*, *Bazel* and custom shell scripts. Please refer to
the relevant files linked below for details about the build commands and used
options.


## Building the toolchain step by step

1. __Install system dependencies__ *(see `apt` in [Dockerfile](https://github.com/Lind-Project/lind-wasm/blob/main/.devcontainer/Dockerfile))*

2. __Download *Clang* and install builtins__ *(see `wget` and `cp` in [Dockerfile](https://github.com/Lind-Project/lind-wasm/blob/main/.devcontainer/Dockerfile))*

    *Clang* supports the *WASI* cross-compilation target out of
    the box, provided the necessary compiler runtime
    [builtins](https://clang.llvm.org/docs/Toolchain.html#compiler-rt-llvm) and
    a matching [sysroot](https://clang.llvm.org/docs/CrossCompilation.html).
    See [*wasi-sdk* docs](https://github.com/WebAssembly/wasi-sdk) for details.

    A pre-built *Clang* can be downloaded from the
    [llvm-project releases](https://github.com/llvm/llvm-project/releases/) page.
    Matching builtins are available in the *lind-wasm* repo under
    [`src/glibc/wasi`](https://github.com/Lind-Project/lind-wasm/tree/main/src/glibc/wasi).

3. __Build *glibc* and generate sysroot__ (see `make_glibc` in [BUILD](https://github.com/Lind-Project/lind-wasm/blob/main/BUILD) file)
    1. Configure and compile *glibc* for the *WASI* target with *Clang*.  (see
      [`wasm-config.sh`](https://github.com/Lind-Project/lind-wasm/blob/main/src/glibc/wasm-config.sh))

    2. Compile extra files:
        - `nptl/pthread_create.c`
        - `lind_syscall/lind_syscall.c`
        - `csu/wasm32/wasi_thread_start.s`
        - `csu/wasm32/set_stack_pointer.s`

    3. Generate sysroot

        Combine the built object files into a single archive file and copy
        along with headers and a pre-built C runtime into a
        sysroot directory structure as required by *Clang*. (see
        [`gen_sysroot.sh`](https://github.com/Lind-Project/lind-wasm/blob/main/src/glibc/gen_sysroot.sh))

4. __Build custom wasmtime__ (see `make_wasmtime` in [BUILD](https://github.com/Lind-Project/lind-wasm/blob/main/BUILD))

       Build with `cargo build` from within `src/wasmtime`. Custom dependencies
       `fdtables`, `RawPOSIX` and `sysdefs` are included in the build automatically
       via Cargo workspace dependencies. (see
       [`Cargo.toml`](https://github.com/Lind-Project/lind-wasm/blob/main/src/wasmtime/Cargo.toml))


A customized `wasm-opt` binary is included in the *lind-wasm* repo under
[`tools/binaryen/bin`](https://github.com/Lind-Project/lind-wasm/blob/main/tools/binaryen/bin)
and can be used as is.


## Automation with Docker

Run the following [*Docker*](https://docs.docker.com/engine/install/) command
from within the *lind-wasm* repo to create a Docker image with the full
toolchain.

```
docker build -t lind-wasm -f .devcontainer/Dockerfile --platform=linux/amd64 .
```

## Next steps

Take a look at how to [compile](compile-programs.md) and [run](run-programs.md)
programs to see the toolchain in action.
