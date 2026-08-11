/* A plain native program. It links libinoutdemo.so and calls functions that
 * transform a caller-allocated buffer IN PLACE, unaware the work happens inside the
 * wasm sandbox (the host stub copies the buffer's current contents into guest
 * memory, runs the call, and copies the result back out).
 *
 * Note `buf` is fully initialized: an in/out argument sends the whole capacity into
 * the guest, so leaving the tail uninitialized would push stack garbage across the
 * sandbox boundary. */
#include <stdio.h>
#include <stddef.h>
#include <string.h>

size_t compact_spaces(char *buf, size_t n);                  /* len = return value   */
void   reverse_str(char *buf, size_t n);                     /* len = NUL-terminated */
void   rot13_block(char *buf, size_t n);                     /* len = whole capacity */
void   append_suffix(char *buf, size_t n,
                     const char *suffix, size_t *out_len);   /* len = out-param      */

int main(void) {
    char buf[64] = {0};

    strcpy(buf, "too   many    spaces");
    size_t w = compact_spaces(buf, sizeof buf);
    printf("compact_spaces -> \"%.*s\" (%zu bytes)\n", (int)w, buf, w);

    strcpy(buf, "sandbox");
    reverse_str(buf, sizeof buf);
    printf("reverse_str    -> \"%s\"\n", buf);

    strcpy(buf, "Hello Lind");
    rot13_block(buf, 10);
    printf("rot13_block    -> \"%.10s\"\n", buf);
    rot13_block(buf, 10);
    printf("rot13_block x2 -> \"%.10s\" (round-trips)\n", buf);

    size_t len = 0;
    strcpy(buf, "lind");
    append_suffix(buf, sizeof buf, "-wasm", &len);
    printf("append_suffix  -> \"%.*s\" (len=%zu)\n", (int)len, buf, len);

    return 0;
}
