#!/bin/bash

/home/lind/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang -pthread --target=wasm32-unknown-wasi --sysroot /home/lind/lind-wasm/src/glibc/sysroot -Wl,--import-memory,--export-memory,--max-memory=1570242560,--export=signal_callback,--export=__stack_pointer,--export=__stack_low,--export=open_grate,--export=close_grate,--export=lseek_grate,--export=read_grate,--export=write_grate,--export-table open_grate.c -g -O0 -o open_grate.wasm && /home/lind/lind-wasm/tools/binaryen/bin/wasm-opt --epoch-injection --asyncify -O2 --debuginfo open_grate.wasm -o open_grate.wasm && /home/lind/lind-wasm/src/wasmtime/target/debug/wasmtime compile open_grate.wasm -o open_grate.cwasm

# ./scripts/lind_run open_grate.wasm tcc.wasm
