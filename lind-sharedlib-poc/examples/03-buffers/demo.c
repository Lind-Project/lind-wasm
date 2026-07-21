/* A plain native program. It links libbufdemo.so and calls functions that write
 * into caller-allocated buffers, unaware the work happens inside the wasm sandbox
 * (the host stub allocates the buffer in guest memory, runs the call, and copies
 * the result back out). Each call exercises a different copy-back-length contract. */
#include <stdio.h>
#include <stddef.h>

size_t to_upper(const char *in, char *out, size_t n);          /* len = return value   */
void   greet(const char *name, char *out, size_t n);          /* len = NUL-terminated  */
void   fill_pattern(char *out, size_t n);                     /* len = whole capacity  */
void   extract_word(const char *in, char *out, size_t n, size_t *out_len); /* len = out-param */

int main(void) {
    char buf[64];

    size_t w = to_upper("hello world", buf, sizeof buf);
    printf("to_upper      -> \"%.*s\" (%zu bytes)\n", (int)w, buf, w);

    greet("lind", buf, sizeof buf);
    printf("greet         -> \"%s\"\n", buf);

    fill_pattern(buf, 10);
    printf("fill_pattern  -> \"%.10s\"\n", buf);

    size_t len = 0;
    extract_word("sandboxed library", buf, sizeof buf, &len);
    printf("extract_word  -> \"%.*s\" (len=%zu)\n", (int)len, buf, len);

    return 0;
}
