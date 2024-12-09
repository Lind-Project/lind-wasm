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

Now let try to print `hello world!` by printf

```
./lindtool.sh cptest PATH_TO_TEST
./lindtool.sh run PATH_TO_TEST
```

A quick-way to get start:
```
docker pull securesystemslab/lind-wasm
docker run -it securesystemslab/lind-wasm /bin/bash
```

New Build System (Bazel):

install bazel on your system: [bazel](https://bazel.build/install)
```
bazel build //:make_all
bazel build //:make_wasmtime
```