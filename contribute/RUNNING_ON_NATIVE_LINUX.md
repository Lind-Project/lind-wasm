# Running on Native Linux

Docker is quick to use, but is not necessary for running Lind-Wasm. Lind-Wasm can be built and run directly inside an Ubuntu 22.04 WSL2
distribution without using Docker.


This guide follows the dependency versions used by
[`Dockerfile.dev`](../Docker/Dockerfile.dev), but installs them directly
inside WSL.

## Tested environment

The following configuration was used to verify these instructions:

Environment #1:
- Windows 10 Pro
- WSL2
- Ubuntu 22.04
- x86-64/AMD64 processor(Ryzen 5 3600)
- 16 GB RAM
- 25 GB of free space

Environment #2:
- Native Linux
- Ubuntu 22.04
- x86-64/AMD64 processor(AMD Ryzen™ AI 9 HX 370)
- 16 GB RAM
- 46 GB of free space

The exact minimum disk-space requirement has not been formally measured.
The dependencies require a significant amount of storage so 15-20gb of free space is recomended to avoid any issues during installation.

## step by step guide on installing without Docker

1. __Install Ubuntu 22.04 on WSL2(SKIP IF YOU ARE ON NATIVE LINUX)__

   From Windows PowerShell, install Ubuntu 22.04 if it is not already
   available:

   ```powershell
   wsl --install -d Ubuntu-22.04
   ```

   Set it as the default WSL distribution:

   ```powershell
   wsl --set-default Ubuntu-22.04
   ```

   Launch Ubuntu 22.04 before continuing.

2. __Install system dependencies__

   Update the packages:

   ```bash
   sudo apt update
   ```

   Install the development packages used by
   [`Dockerfile.dev`](../Docker/Dockerfile.dev):

   ```bash
   sudo apt install -y \
     binutils \
     bison \
     cmake \
     flex \
     build-essential \
     ca-certificates \
     strace \
     curl \
     gawk \
     git \
     gnupg \
     libc6-dev-i386-cross \
     libxml2 \
     make \
     python3 \
     sed \
     sudo \
     unzip \
     zip \
     autoconf \
     rsync \
     libtool \
     automake \
     vim \
     wget \
     openssl \
     libssl-dev \
     golang \
     gdb \
     linux-tools-common \
     linux-tools-generic \
     tzdata \
     xz-utils
   ```

   Install the pinned `libtinfo5` compatibility package:

   ```bash
   wget http://security.ubuntu.com/ubuntu/pool/universe/n/ncurses/libtinfo5_6.3-2ubuntu0.2_amd64.deb
   sudo apt install -y ./libtinfo5_6.3-2ubuntu0.2_amd64.deb
   rm libtinfo5_6.3-2ubuntu0.2_amd64.deb
   ```

3. __Install Rust__

   Install Rust through `rustup` using the minimal profile:

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
     sh -s -- -y --profile minimal
   ```

   Load the Rust environment:

   ```bash
   source "$HOME/.cargo/env"
   ```

   Install the pinned nightly toolchain and `rust-src` component:

   ```bash
   rustup toolchain install nightly-2026-02-11 \
     --profile minimal \
     --component rust-src
   ```

   Verify the installation:

   ```bash
   rustc +nightly-2026-02-11 --version
   ```

4. __Install WABT 1.0.38__

   Download and build the pinned WABT release:

   ```bash
   curl -fsSL \
     https://github.com/WebAssembly/wabt/releases/download/1.0.38/wabt-1.0.38.tar.xz \
     -o /tmp/wabt.tar.xz

   mkdir -p /tmp/wabt-src

   tar -xJf /tmp/wabt.tar.xz \
     -C /tmp/wabt-src \
     --strip-components=1

   cmake -S /tmp/wabt-src \
     -B /tmp/wabt-src/build \
     -DCMAKE_BUILD_TYPE=Release \
     -DBUILD_TESTS=OFF

   cmake --build /tmp/wabt-src/build \
     --target wasm2wat wat2wasm wasm-objdump \
     --parallel "$(nproc)"
   ```

   Install the required programs:

   ```bash
   sudo install -m 0755 \
     /tmp/wabt-src/build/wasm2wat \
     /usr/local/bin/wasm2wat

   sudo install -m 0755 \
     /tmp/wabt-src/build/wat2wasm \
     /usr/local/bin/wat2wasm

   sudo install -m 0755 \
     /tmp/wabt-src/build/wasm-objdump \
     /usr/local/bin/wasm-objdump
   ```

   Remove the temporary build files:

   ```bash
   rm -rf /tmp/wabt.tar.xz /tmp/wabt-src
   ```

   Verify the tools:

   ```bash
   wasm2wat --version
   wat2wasm --version
   wasm-objdump --version
   ```

5. __Clone Lind-Wasm__

   Clone the repository into the Linux filesystem rather than under `/mnt/c`:

   ```bash
   cd ~
   git clone https://github.com/Lind-Project/lind-wasm.git
   cd lind-wasm
   ```

6. __Install Clang/LLVM 18.1.8__

   Download and extract the pinned LLVM package:

   ```bash
   cd ~/lind-wasm

   curl -fsSL \
     https://github.com/llvm/llvm-project/releases/download/llvmorg-18.1.8/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04.tar.xz \
     -o /tmp/llvm.tar.xz

   tar -xJf /tmp/llvm.tar.xz -C ~/lind-wasm
   rm /tmp/llvm.tar.xz
   ```

7. __Install Lind's WASI files and configure the environment__

   Copy Lind's WASI files into the Clang resource directory:

   ```bash
   cp -r \
     ~/lind-wasm/src/glibc/wasi \
     ~/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/lib/clang/18/lib/
   ```

   Add the Lind-Wasm root, LLVM, and Rust directories to the shell
   environment:

   ```bash
   echo 'export LIND_WASM_ROOT="$HOME/lind-wasm"' >> ~/.bashrc
   echo 'export CLANG="$LIND_WASM_ROOT/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04"' >> ~/.bashrc
   echo 'export PATH="$CLANG/bin:$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
   source ~/.bashrc
   ```

   Verify LLVM:

   ```bash
   clang --version
   wasm-ld --version
   ```

8. __Build Lind-Wasm__

   From the repository root, build the development runtime, Lind filesystem,
   custom glibc, and sysroot:

   ```bash
   cd ~/lind-wasm
   make lind-debug
   ```

   See the project [`Makefile`](../Makefile) for the individual build
   targets.

9. __Install the Lind helper commands__

   Make the scripts executable:

   ```bash
   chmod +x \
     scripts/bin/lind_compile \
     scripts/bin/lind_run \
     scripts/bin/cargo-lind_compile
   ```

   Create commands in `/usr/local/bin`:

   ```bash
   sudo ln -sfn \
     "$HOME/lind-wasm/scripts/bin/lind_compile" \
     /usr/local/bin/lind_compile

   sudo ln -sfn \
     "$HOME/lind-wasm/scripts/bin/lind_run" \
     /usr/local/bin/lind_run

   sudo ln -sfn \
     "$HOME/lind-wasm/scripts/bin/cargo-lind_compile" \
     /usr/local/bin/cargo-lind_compile
   ```

   Add the alternate command names used by the development image:

   ```bash
   sudo ln -sfn /usr/local/bin/lind_compile /usr/local/bin/lind-clang
   sudo ln -sfn /usr/local/bin/lind_run /usr/local/bin/lind-wasm
   sudo ln -sfn \
     /usr/local/bin/cargo-lind_compile \
     /usr/local/bin/lind-cargo-build
   ```

   Verify the commands:

   ```bash
   command -v lind_compile
   command -v lind_run
   command -v cargo-lind_compile
   ```

10. __Create the Lind filesystem compatibility link__

    The current runtime expects `lindfs` at
    `/home/lind/lind-wasm/lindfs`. When the WSL username is not `lind`,
    create a compatibility symbolic link:

    ```bash
    sudo mkdir -p /home/lind
    sudo ln -sfnT "$HOME/lind-wasm" /home/lind/lind-wasm
    ```

    Verify the path:

    ```bash
    ls -ld /home/lind/lind-wasm/lindfs
    ```

11. __Compile and run a test program__

    Create `hello.c` in the repository root:

    ```c
    #include <stdio.h>

    int main(void) {
        printf("Hello, World!\n");
        return 0;
    }
    ```

    Compile it:

    ```bash
    cd ~/lind-wasm
    lind_compile -s hello.c
    ```

    Run the compiled program using its path inside `lindfs`:

    ```bash
    lind_run /hello.cwasm
    ```

    The expected output is:

    ```text
    Hello, World!
    ```
