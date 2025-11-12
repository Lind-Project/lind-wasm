clang \
    --target=wasm32-unknown-wasi \
    -fPIC \
    --sysroot /home/lind/lind-wasm/src/glibc/sysroot \
    -fvisibility=default \
    -Wl,--import-memory \
    -Wl,--shared-memory \
    -Wl,--max-memory=67108864 \
    -Wl,--no-entry \
    -Wl,--export-dynamic \
    -Wl,--export=myfunc \
    -Wl,-pie \
    lib.c -g -O0 -o lib.wasm

# wasm-opt --debuginfo --set-globals --pass-arg=set-globals@__tls_base=40960000 lib.wasm -o lib.wasm
