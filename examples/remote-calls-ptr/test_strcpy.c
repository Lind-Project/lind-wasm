#include <stdio.h>
#include <string.h>

// strcpy(dest, src) — intercepted by remote-lib.
// src is sent to lind-remote-server which calls native strcpy(), then
// the filled dest buffer is written back into WASM linear memory.
// The return value (native char *) is discarded to avoid WASM pointer confusion.
int main(void) {
    char dest[64] = {0};
    const char *src = "hello from remote strcpy!";
    strcpy(dest, src);
    printf("dest: %s\n", dest);
    return 0;
}
