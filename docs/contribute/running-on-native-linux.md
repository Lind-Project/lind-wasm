# Running on Native Linux

Docker is quick to use, but is not necessary for running Lind-Wasm. Lind-Wasm can be built and run directly on Ubuntu 22.04, both in WSL2
and on native Linux, without using Docker.


This guide follows the dependency versions used by
[`Dockerfile.dev`](https://github.com/Lind-Project/lind-wasm/blob/main/Docker/Dockerfile.dev), but installs them directly
in the WSL2 or native Linux environment.

## Tested environment

The following configuration was used to run lind-wasm on WSL2 and native Linux

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
The dependencies require a significant amount of storage so 15-20gb of free space is recommended to avoid any issues during installation.

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
   [`Dockerfile.dev`](https://github.com/Lind-Project/lind-wasm/blob/main/Docker/Dockerfile.dev):

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

   The repository pins its toolchain in
   [`rust-toolchain.toml`](https://github.com/Lind-Project/lind-wasm/blob/main/rust-toolchain.toml),
   and `rustup` installs and selects that channel automatically the first time you
   build inside the repository. Do not pin a different nightly by hand:
   `rust-toolchain.toml` wins at build time, so a toolchain you select manually —
   and any components you add to it — is silently ignored.

   The `rust-src` component still has to be added explicitly. Because that must
   happen against the pinned channel, it is done after cloning, in step 8.

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

   From the repository root, add `rust-src` to the pinned toolchain. Running this
   inside the repository is what makes `rustup` resolve `rust-toolchain.toml`:

   ```bash
   cd ~/lind-wasm
   rustup component add rust-src
   rustup show active-toolchain
   ```

   Check that the machine has everything else it needs. Every `FAIL` comes with
   the command that fixes it:

   ```bash
   make checkenv BUILD_ONLY=1
   ```

   Then build the runtime, Lind filesystem, custom glibc, and sysroot:

   ```bash
   make build
   ```

   Use `make build` rather than the individual targets: `make lind-boot sysroot`
   skips the `lindfs` target, and `make sysroot` on its own fails on a clean
   checkout because building the shared libc needs `lind-boot` to exist already.
   For a debug runtime use `make lind-debug` instead — but do not mix the two, or
   you will get `unknown import: debug::lind_debug_num` at run time.

   See the project [`Makefile`](https://github.com/Lind-Project/lind-wasm/blob/main/Makefile) for the individual build
   targets and build knobs.

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

    The runtime `chroot`s into a path that is compiled in as a constant —
    `LINDFS_ROOT` in `src/sysdefs/src/constants/lind_platform_const.rs`, currently
    `/home/lind/lind-wasm/lindfs`. It is not read from the environment, so a
    checkout anywhere else panics at startup with `The configured lindfs does not
    exist`. Unless your home directory is literally `/home/lind`, create a
    compatibility symbolic link:

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
    lind_compile hello.c
    ```

    Run the compiled program using its path inside `lindfs`:

    ```bash
    lind_run /hello.cwasm
    ```

    The expected output is:

    ```text
    Hello, World!
    ```

    Two things to expect the first time:

    - `lind_run` prompts for a password. It re-executes itself under `sudo -E`
      because the runtime needs root to `chroot`.
    - The path is `/hello.cwasm`, not `./hello.cwasm`. `lind_compile` copies its
      output into `lindfs/`, and the runtime `chroot`s there, so paths are relative
      to `lindfs` rather than to your shell's working directory.

    `lind_compile` builds dynamically by default, resolving *lind-glibc* from
    `lindfs/lib/libc.cwasm` at run time. Pass `-s` for a statically linked binary,
    which does not need the shared libc to be present.

12. __Run the full test suite__

    From the repository root, run the full test suite. It can take a while, so
    wait for it to finish:

    ```bash
    cd ~/lind-wasm
    make test
    ```
