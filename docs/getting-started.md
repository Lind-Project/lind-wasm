# Getting Started

1. Make sure to read the [Basics](index.md) first.
2. If you want to start contributing, check out the [Contributor Instructions](contribute/README.md).
3. Continue reading to run a *__Hello World__* program in the Lind Sandbox.

## Hello World!

**1. Set up the environment**

Run the following commands in your terminal to download and shell into an environment
that comes with the Lind Sandbox. *You'll need [Docker installed](https://docs.docker.com/engine/install/).*

```
docker pull --platform=linux/amd64 securesystemslab/lind-wasm  # this might take a while ...
docker run --platform=linux/amd64 -it securesystemslab/lind-wasm /bin/bash
```

There is a development environment with tooling and source code available, instructions to be found [here](contribute/dev-container.md)

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

**3. Compile and run**

Use lind scripts to compile and run your program in the Lind Sandbox.

```bash
lind_compile hello.c
lind_run hello.cwasm
```

*Here is what happens under the hood:*

1.  `lind_compile` compiles `hello.c` into a WebAssembly (WASM)
binary using headers etc. from *lind-glibc*.
1. `lind_run` runs the compiled wasm using *lind-wasm* runtime
and the *lind-posix* microvisor.

## What's next!

The Lind documentation is currently under heavy construction. Please [submit an
issue](contribute/README.md), if something doesn't seem right or is missing.
More detailed usage guides will follow soon!
