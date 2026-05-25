#include <string.h>

/*
 * Thin wrapper loaded by lind-remote-server via dlopen.
 * Receives host-side pointers allocated by the server, not WASM addresses.
 */
char *remote_strcpy(char *dest, const char *src) {
    return strcpy(dest, src);
}
