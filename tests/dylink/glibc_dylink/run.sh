# /home/lind/lind-wasm/src/wasmtime/target/debug/wasmtime run --wasi threads=y --wasi preview2=n --preload env=/home/lind/lind-wasm/src/glibc/sysroot/lib/wasm32-wasi/libc.wasm main.wasm
# /home/lind/lind-wasm/src/wasmtime/target/debug/wasmtime run --wasi threads=y --wasi preview2=n main.wasm
# /home/lind/lind-wasm/src/wasmtime/target/debug/wasmtime run --wasi threads=y --wasi preview2=n --preload env=/home/lind/outsource/libc.so --preload env=/home/lind/outsource/libm.so main.wasm
sudo /home/lind/lind-wasm/src/lind-boot/target/debug/lind-boot --preload env=/lib/libc.cwasm test/main.cwasm