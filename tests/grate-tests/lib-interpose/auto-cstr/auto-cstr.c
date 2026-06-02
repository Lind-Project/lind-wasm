// Cage for auto-cstr marshalling test.
// Calls strlen("hello"); grate intercepts with LIND_SIZE_CSTR and returns
// len*2 (10) instead of 5 to prove the string was copied into shadow memory.
#include <stdio.h>
#include <string.h>

extern size_t strlen(const char *s);

// Global so the compiler cannot constant-fold strlen at compile time.
static char g_s[] = "hello";

int main(void) {
    size_t result = strlen(g_s);
    if (result != 10) {
        fprintf(stderr, "[Cage|auto-cstr] FAIL: strlen(\"%s\") = %zu, expected 10\n",
                g_s, result);
        return 1;
    }
    printf("[Cage|auto-cstr] PASS: strlen(\"%s\") = %zu (intercepted as len*2)\n",
           g_s, result);
    return 0;
}
