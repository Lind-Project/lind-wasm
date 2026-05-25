#include <stdio.h>
#include <string.h>

// Custom remote handler for strcpy.
// Called by lind-remote-server with real native pointers — dest and src are
// host-side buffers allocated by the server, not WASM addresses.
char *custom_strcpy(char *dest, const char *src) {
    fprintf(stderr, "[remote-handler] custom_strcpy invoked: src=\"%s\"\n", src);
    char *result = strcpy(dest, src);
    fprintf(stderr, "[remote-handler] custom_strcpy done: dest=\"%s\"\n", dest);
    return result;
}
