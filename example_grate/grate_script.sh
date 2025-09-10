#!/bin/bash
# /home/lind/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/bin/clang -pthread \
#     --target=wasm32-unknown-wasi --sysroot /home/lind/lind-wasm/src/glibc/sysroot \
#     -Wl,--import-memory,--export-memory,--max-memory=67108864,--export=__stack_pointer,--export=__stack_low,--export=geteuid_grate geteuid_grate.c \
#     -g -O0 -o geteuid_grate.wasm \
#     && tools/binaryen/bin/wasm-opt --asyncify --epoch-injection --debuginfo geteuid_grate.wasm -o geteuid_grate.wasm \
#     && /home/lind/lind-wasm/src/wasmtime/target/release/wasmtime compile geteuid_grate.wasm -o geteuid_grate.cwasm

/home/lind/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/bin/clang -pthread \
    --target=wasm32-unknown-wasi --sysroot /home/lind/lind-wasm/src/glibc/sysroot \
    -Wl,--import-memory,--export-memory,--max-memory=67108864,--export=__stack_pointer,--export=__stack_low,--export=open_grate open_grate.c \
    -g -O0 -o open_grate.wasm \
    && tools/binaryen/bin/wasm-opt --asyncify --epoch-injection --debuginfo open_grate.wasm -o open_grate.wasm \
    && /home/lind/lind-wasm/src/wasmtime/target/release/wasmtime compile open_grate.wasm -o open_grate.cwasm
