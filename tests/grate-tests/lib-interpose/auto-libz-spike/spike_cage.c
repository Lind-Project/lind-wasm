// Spike cage: calls the REAL adler32 (interposed → marshalled → grate's real libz).
// Known test vector: adler32(1, "Wikipedia", 9) == 0x11E60398.
#include <stdio.h>

extern unsigned long adler32(unsigned long adler, const unsigned char *buf, unsigned int len);

// Global so the buffer is in the data segment (cross-cage reachable), not the stack.
static const unsigned char g_data[] = "Wikipedia";

int main(void) {
    fprintf(stderr, "[Cage|spike] before adler32 call\n");
    unsigned long a = adler32(1, g_data, 9);
    fprintf(stderr, "[Cage|spike] after adler32 call: 0x%lx\n", a);
    if (a != 0x11E60398UL) {
        fprintf(stderr, "[Cage|spike] FAIL: adler32=0x%lx, expected 0x11E60398\n", a);
        return 1;
    }
    printf("[Cage|spike] PASS: adler32(\"Wikipedia\")=0x%lx\n", a);
    return 0;
}
