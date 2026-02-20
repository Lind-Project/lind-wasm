# /home/lind/lind-wasm/src/wasmtime/target/debug/wasmtime run --wasi threads=y --wasi preview2=n --preload env=/home/lind/lind-wasm/src/glibc/sysroot/lib/wasm32-wasi/libc.wasm main.wasm
# /home/lind/lind-wasm/src/wasmtime/target/debug/wasmtime run --wasi threads=y --wasi preview2=n main.wasm
/home/lind/lind-wasm/src/wasmtime/target/debug/wasmtime run --wasi threads=y --wasi preview2=n --preload env=/lib/libc.so --preload env=/lib/libm.so main.wasm
