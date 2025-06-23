#!/bin/bash

set -e

BUILDDIR=build
mkdir -p $BUILDDIR
cd $BUILDDIR

../configure --disable-werror --disable-hidden-plt --disable-profile --prefix=$PWD/target \
    --with-headers=/usr/i686-linux-gnu/include \
    --host=i686-linux-gnu --build=i686-linux-gnuu \
    CFLAGS=" -matomics -mbulk-memory -O2 -g" \
    CC="${CLANG:=/home/alice/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04}/bin/clang --target=wasm32-unknown-wasi -v -Wno-int-conversion"
