#!/bin/bash

set -x 

cd /home/lind/lind-wasm/

src/wasmtime/target/debug/wasmtime run --env PRELOADS="$(cat imfs_grate/preloads)" --allow-precompiled --wasi threads=y --wasi preview2=n imfs_grate/open_grate.wasm tcc.wasm
