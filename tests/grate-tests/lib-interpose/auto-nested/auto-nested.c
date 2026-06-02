// Cage for auto-nested marshalling test.
// Calls toy_buf_checksum({data, len}) from libtoy.
// The grate intercepts with a nested struct spec: the struct's `data` pointer
// field is automatically copied in. Handler computes sum+1 (to prove interception
// and that `data` bytes are locally accessible).
#include <stdio.h>

struct toy_buffer { char *data; unsigned len; };

extern int toy_buf_checksum(const struct toy_buffer *b);

int main(void) {
    // "ABC" = 65 + 66 + 67 = 198. Handler returns sum + 1 = 199.
    static char g_data[] = "ABC";
    struct toy_buffer buf = { g_data, 3 };

    int result = toy_buf_checksum(&buf);
    if (result != 199) {
        fprintf(stderr, "[Cage|auto-nested] FAIL: toy_buf_checksum = %d, expected 199\n",
                result);
        return 1;
    }
    printf("[Cage|auto-nested] PASS: toy_buf_checksum = %d (sum+1)\n", result);
    return 0;
}
