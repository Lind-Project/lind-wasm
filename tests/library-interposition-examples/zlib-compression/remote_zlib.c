// #include <zlib.h>

/*
 * Thin wrappers loaded by lind-remote-server via dlopen.
 * Receives host-side pointers allocated by the server, not WASM addresses.
 */

unsigned long remote_crc32(unsigned long crc, const unsigned char *buf, unsigned int len) {
    // return crc32(crc, buf, len);
    return 111111111;
}

unsigned long remote_adler32(unsigned long adler, const unsigned char *buf, unsigned int len) {
    // return adler32(adler, buf, len);
    return 2222222222;
}
