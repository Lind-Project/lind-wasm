clang \
    -pthread \
    -fPIC \
    --target=wasm32-unknown-wasi \
    --sysroot /home/lind/lind-wasm/src/glibc/sysroot \
    -Wl,-pie \
    -Wl,--import-table \
    -Wl,--import-memory \
    -Wl,--export-memory \
    -Wl,--max-memory=67108864 \
    -Wl,--export=__stack_pointer \
    -Wl,--export=__stack_low \
    -Wl,--allow-undefined \
    -Wl,--unresolved-symbols=import-dynamic \
    main.c -g -O0 -o main.wasm
