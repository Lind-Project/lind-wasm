// Placeholder cage for harness compatibility.
// The real cage for this test is the Python interpreter; run manually:
//   lind-wasm --preload env=/lib/libz.so \
//             --preload env=/lib/libpython3.14.so \
//             grates/zlib-python_grate.cwasm \
//             /usr/local/bin/python /test-zlib.py
int main(void) { return 0; }
