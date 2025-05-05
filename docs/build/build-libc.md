# Building glibc to WASM

I'd recommend reading this doc in its entirety before trying to compile.

## Prerequisites

You need to have an access to the server, ask somebody to help you. Then you pull an image to docker and run it

```
docker pull ubuntu:22.04
docker run -it ubuntu
```

Update apt and apt-get 

```
cd home
apt-get update
apt update
```

we should install some apt essential

```
apt install build-essential
apt install git
apt install wget
apt install gcc-i686-linux-gnu g++-i686-linux-gnu
apt install nano
apt install bison
apt install gawk
apt install vim
apt install libxml2
apt install python3 (2.America 105.New York)
```

We need glibc from lind-wasm, if you did it already then ignore it
[https://github.com/Lind-Project/lind-wasm.git](https://github.com/Lind-Project/lind-wasm.git)

We need WASM compatible `clang` and `ar`, which can be built locally from `wasi-sdk`
[https://github.com/WebAssembly/wasi-sdk](https://github.com/WebAssembly/wasi-sdk)

Also strongly recommend to install `wasm-objdump` from the `wabt` toolkit
[https://github.com/WebAssembly/wabt](https://github.com/WebAssembly/wabt)

If you want to download files from github to server use `git clone` and `recurse-submodules`deal with repositories that contain submodules

```
git clone --recurse-submodules
```

If any error said "permission denied" then just add "sudo" at the front of the command line.

If you want to edit file through terminal, try to search vim and study how to use it.

## Two ways to install complier
1. Download `clang-16` and add some file(I recommend using this way, and I will use clang-16 as example for explanation)
2. Git clone wasi-sdk and use the `clang-18` in wasi-sdk, you need to compile wasi-sdk before using it


### Install clang-16(recommended way)
Download `clang-16` from this link

```
wget https://github.com/llvm/llvm-project/releases/download/llvmorg-16.0.4/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz
```

Unzip clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz

```
tar -xf clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz
```

Switch branch of glibc(cd to lind-wasm/glibc). Find out which branch you are on currently and switch to branch "main" 

```
cd /home/lind-wasm/glibc
git branch -a
git switch main 
```

We move `libclang_rt.builtins-wasm32.a` from `/home/lind-wasm/glibc/wasi` to `/home/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/lib/clang/16/lib/` using

```
cd /home
mv /home/lind-wasm/glibc/wasi /home/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/lib/clang/16/lib
```

### Compile wasi-sdk(not available now)
I assume you already git clone wasi-sdk, now you need to cd to wasi-sdk and run these code

```
cmake -G Ninja -B build/toolchain -S . -DWASI_SDK_BUILD_TOOLCHAIN=ON -DCMAKE_INSTALL_PREFIX=build/install

cmake --build build/toolchain --target install

cmake -G Ninja -B build/sysroot -S . \
    -DCMAKE_INSTALL_PREFIX=build/install \
    -DCMAKE_TOOLCHAIN_FILE=build/install/share/cmake/wasi-sdk.cmake \
    -DCMAKE_C_COMPILER_WORKS=ON \
    -DCMAKE_CXX_COMPILER_WORKS=ON
cmake --build build/sysroot --target install
```

## Configure
We create a .sh file and write a config script in the file. We use `nano` to create file in the glibc root directory(glibc is in the lind-wasm directory) and you can change `anyname` into the filename you want

```
cd /home/lind-wasm/glibc
nano anyname.sh
```

then we write the script like this

```
#!/bin/bash
set -e
BUILDDIR=build
mkdir -p $BUILDDIR
cd $BUILDDIR
../configure --disable-werror --disable-hidden-plt --with-headers=/usr/i686-linux-gnu/include --prefix=/home/lind-wasm/glibc/target --host=i686-linux-gnu --build=i686-linux-gnu\
    CFLAGS=" -O2 -g" \
    CC="/home/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang-16 --target=wasm32-unkown-wasi -v -Wno-int-conversion"
```

You should replace `CC` to the path to your `clang`, this path should work but if not change this part `/home/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang-16` into your own path. If you define `BUILDDIR=build`, then the compiled WASM object files will appear under `glibc/build`.
Be aware that you should make sure this build directory is empty before running config script, so you need to `rm -rf build` to remove all the things inside build directory before recompiling it.

A crutial job of the configure script is deciding which sysdeps directories to use according to the `host` and `build` string.
We already changed the configure script in glibc root directory, and the lind add-on directories are already baked to be included.

The configure flags we need:

- `disable-werror`: we have countless warnings, so we ignore them for now
- `disable-hidden-plt`: PLT bypassing optimization is causing ~50k errors, simply disable it for now
- `with-headers`: glibc requires Linux kernel headers to be installed before config and compile, so set this flag to a built-in sysroot of 32bit, this doesn't seem to raise an issue for our WASM built
- `prefix=`: this is the path of the generated sysroot when you use `make install`. But note that, the glibc's `make install` will NOT work at all for WASM, because WASM sysroot has differen structure convention, also requires an `llvm-ar` arhive. More details, see my script `gen_sysroot.sh`. However, we can still use `make install` just to generate the `.h` files of the sysroot
- `host` & `target`: we start off from the sysdeps direcotries of i686, so fixing these options

The compiler flags we need:

- `-O2 -g`: nah the glibc won't allow you to compile with `O0`, so we bear with this `O2` optimization during debugging. But sometimes you can change to `O1`.
-  `-Wno-int-conversion`: we disable int conversion warnings, cuz all 32bit types as WASM function arguments, are eventually i32 anyway
- `--target=wasm32-unkown-wasi`: this tells the compiler we want to compile to WASM

Now the last step is to run the .sh file

```
sudo chmod +x anyname.sh 
./anyname.sh
```

After config succeed, you will see these in the `build` directory,

```
Makefile  bits  config.h  config.log  config.make  config.status
```

## Compiling to object files
In the build directory, usually we use `make --keep-going -j4`. The first flag is to continue compiling after errors, we need this cuz there are too many errors now (mainly due to assembly about threading). The `-j` is important to speed it up, but also makes the compilation log interleaved. The compilation log is **VERY IMPORTANT**, which tells why a given c file failed to be compiled. So sometimes we don't want the `-j`. Also, we can copy the actual compiler command in the compile log. For such commands, if we want to compile a single C file, only the source file path need to be further specified. We can use this to test compiling a specific file.

```
cd build
make --keep-going -j4
```

## Installing glibc
We need to install the glibc we complied to the prefix in the .sh file. For example, mine will install into `target`. This is the install command line we need to use

```
make install --keep-going
```

## Generating WASM sysroot
Back to glibc

```
cd ..
```

This is an example for `gen_sysroot.sh`

```
#!/bin/bash

# Define the source directory for object files (change ./build to your desired path)
src_dir="./build"

# Define paths for copying additional resources
include_source_dir="/home/lind-wasm/glibc/target/include"
crt1_source_path="/home/lind-wasm/glibc/lind_syscall/crt1.o"
lind_syscall_path="/home/lind-wasm/glibc/lind_syscall/lind_syscall.o" # Path to the lind_syscall.o file

# Define the output archive and sysroot directory
output_archive="sysroot/lib/wasm32-wasi/libc.a"
sysroot_dir="sysroot"

# First, remove the existing sysroot directory to start cleanly
rm -rf "$sysroot_dir"

# Find all .o files recursively in the source directory, ignoring stamp.o
object_files=$(find "$src_dir" -type f -name "*.o" ! \( -name "stamp.o" -o -name "argp-pvh.o" -o -name "repertoire.o" \))

# Add the lind_syscall.o file to the list of object files
object_files="$object_files $lind_syscall_path"

# Check if object files were found
if [ -z "$object_files" ]; then
  echo "No suitable .o files found in '$src_dir'."
  exit 1
fi

# Create the sysroot directory structure
mkdir -p "$sysroot_dir/include/wasm32-wasi" "$sysroot_dir/lib/wasm32-wasi"

# Pack all found .o files into a single .a archive
/home/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/llvm-ar rcs "$output_archive" $object_files

# Check if llvm-ar succeeded
if [ $? -eq 0 ]; then
  echo "Successfully created $output_archive with the following .o files:"
  echo "$object_files"
else
  echo "Failed to create the archive."
  exit 1
fi

# Copy all files from the external include directory to the new sysroot include directory
cp -r "$include_source_dir"/* "$sysroot_dir/include/wasm32-wasi/"

# Copy the crt1.o file into the new sysroot lib directory
cp "$crt1_source_path" "$sysroot_dir/lib/wasm32-wasi/"
```
Here are some macros we need to twist:

- `src_dir`: the glibc `build` directory that contains all the WASM object files
- `include_source_dir`: the path to your pre-built headers
- `crt1_source_path`: path to your pre-built crt1.o
- `lind_syscall_path`: you also need to pre-compile `lind_syscall.o`, just like `crt1.o`, and the source file is under glibc/lind_syscall
- `sysroot_dir`: path to generate the sysroot at
- `output_archive`: the path to the generate the libc.a, should be align with `sysroot_dir`
  
Note that the header files should be pre-generated using `make install`. The crt1.o should be pre-compiled from this simple C file (see the WASM compile doc as well). The main job of this script is to change `include_source_dir`(path to target file), `crt1_source_path`(path to crt1.o), `lind_syscall_path`(path to lind_syscall.o), and `Pack all found .o files into a single .a archive`(path to llvm-ar) into your own path. crt1.o and lind_syscall.o are in `lind_syscall` directory in glibc.

After modifying all the path talked on the above, try to run `gen_sysroot.sh` and see if it works
```
chmod +x gen_sysroot.sh
./gen_sysroot.sh
```

This procedure is specified in the `gen_sysroot.sh` script in our glibc repo. It's main job is to generate a WASM sysroot structre like

```
sysroot/
- include/
  - wasm32-wasi/
    - stdio.h
    - ...other headers
- lib/
  - wasm32-wasi/
    - crt1.o
    - libc.a
```
