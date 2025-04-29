# Building Wasmtime

## Prerequisites

### location
Just be aware Wasmtime is in the lind-wasm directory.

### Git Submodules
The Wasmtime repository contains a number of git submodules. To build Wasmtime and most other crates in the repository, ensure that these are initialized with the following command:
```sh
cd /home/lind-wasm/wasmtime
git submodule update --init
```
### Switch branch
switch branch to add-lind

```
git switch add-lind
```

### The Rust Toolchain
You should have these tools, if not install the Rust toolchain, which includes `rustup`, `cargo`, `rustc`, etc. You can find the installation instructions [here](https://www.rust-lang.org/).

### libclang (Optional)
The `wasmtime-fuzzing` crate transitively depends on `bindgen`, which requires `libclang` to be installed on your system. If you want to work on Wasmtime's fuzzing infrastructure, you'll need `libclang`. Details on how to get `libclang` and make it available for `bindgen` are [here](https://rust-lang.github.io/rust-bindgen/requirements.html).

## Building the Wasmtime CLI
```
cd /home/lind-wasm/wasmtime/crates/rustposix/src
```

Cd to path `/home/lind-wasm/wasmtime/crates/rustposix/src` and vim file `build.rs` change the first line into `cargo:rustc-link-search=native=/home/lind-wasm/wasmtime/crates/rustposix`

```
vim build.rs
```

```
cd /home/lind-wasm/wasmtime
```

Remember to export

```
export LD_LIBRARY_PATH=/home/lind-wasm/wasmtime/crates/rustposix:$LD_LIBRARY_PATH
```

You should find librustposix.so in rustposix(...wasmtime/crates/rustposix), but instead we should replace librustposix.so with another librustposix.so located in the safeposix-rust you complied before. Use the code below, you can change `librustposix.so` with the name of the file you want to replace and change `/home/lind-wasm/wasmtime/crates/rustposix` with the path of the new file(for file use cp for directory use cp -r )

```
cp /home/safeposix-rust/target/debug/librustposix.so /home/lind-wasm/wasmtime/crates/rustposix
```

To make an unoptimized, debug build of the Wasmtime CLI tool, go to the root of the repository and run:
```sh
cargo build
```
The built executable will be located at `target/debug/wasmtime`.

To make an optimized build, run the following command in the root of the repository:
```sh
cargo build --release
```
The built executable will be located at `target/release/wasmtime`.

You can also build and run a local Wasmtime CLI by replacing `cargo build` with `cargo run`.


Additional Instructions:

First you need to download lind-wasm in your docker to home directory

```
sudo git clone https://github.com/Lind-Project/lind-wasm.git
```

I assume you have rust else use

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs/ | sh
rustup install nightly
. "$HOME/.cargo/env"
rustup default nightly
```

set Clang path
```
wget https://github.com/llvm/llvm-project/releases/download/llvmorg-16.0.4/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz
tar -xf clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz
export CLANG=clang_folder
```

```
cd lind-wasm
mv ./src/glibc/wasi $CLANG/lib/clang/16/lib
./lindtool.sh make_all
./lindtool.sh compile_wasmtime
```

