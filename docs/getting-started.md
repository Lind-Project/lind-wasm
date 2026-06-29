# Getting Started

1. Make sure to read the [Basics](index.md) first.
2. If you want to start contributing, check out the [Contributor Instructions](contribute/index.md).
3. Continue reading to run a *__Hello World__* program in the Lind Sandbox.

## Hello World!

**1. Set up the environment**

Run the following commands in your terminal to download and shell into an environment
that comes with the Lind Sandbox. *You'll need [Docker installed](https://docs.docker.com/engine/install/).*

```
docker pull --platform=linux/amd64 securesystemslab/lind-wasm-dev  # this might take a while ...
docker run --platform=linux/amd64 -it --privileged --ipc=host --init --cap-add=SYS_PTRACE securesystemslab/lind-wasm-dev /bin/bash
cd lind-wasm
```

This is a development environment with tooling and source code available. Additional instructions can be found [here](contribute/dev-container.md).

**2. Write a program**

In the same terminal, use e.g. `vi` to write a `hello.c` program to be executed
in the Lind sandbox. You can also just paste the snippet below.

```bash
cat << EOF > hello.c
#include <stdio.h>

int main() {
    printf("Hello, World!\n");
    return 0;
}
EOF
```

**3. Compile the Lind-Wasm runtime**

The Lind-Wasm runtime must be compiled before running the program. Use this path for a first build:

```bash
make lind-boot sysroot
```

This builds the runtime (`lind-boot`, Wasmtime, RawPOSIX, 3i, etc.) and the lind-glibc sysroot.

For a full build, including both lind-glibc and Rust code, use `make all`. More build targets are documented in `lind-wasm/Makefile`.

**4. Compile and run**

Inside the development container, `lind-clang` is an alias for `scripts/lind_compile`, and `lind-wasm` is an alias for `scripts/lind_run`. If you are running outside the container, use the scripts directly or add the aliases to your shell.

```bash
lind-clang hello.c
lind-wasm hello.cwasm
```

*Here is what happens under the hood:*

1.  `lind-clang`(aka `scripts/bin/lind_compile`) compiles `hello.c` into a WebAssembly (WASM)
binary that is linked against *lind-glibc*, and put into lind file system root(`lind-wasm/lindfs`).
1. `lind-wasm`(aka `scripts/bin/lind_run`) runs the compiled wasm using *Lind-Wasm* runtime
and the *RawPOSIX* microvisor.

--- 

To compile a Rust crate into a *lind-glibc* linked WASM binary, follow this guide: [Compiling Rust Code with `lind-glibc`](./contribute/compile-with-rust.md)

## What's next!

The Lind documentation is currently under heavy construction. Please [submit an
issue](https://github.com/Lind-Project/lind-wasm/issues), if something doesn't seem right or is missing.
More detailed usage guides will follow soon!
