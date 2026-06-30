// Cage: calls toy_argv_len on a NULL-terminated argv (interposed → grate marshals
// the whole pointer array + each string). "hello"+"world"+"!" = 5+5+1 = 11.
#include <stdio.h>

extern int toy_argv_len(const char **argv);

static const char *args[] = { "hello", "world", "!", 0 };

int main(void) {
    int n = toy_argv_len(args);
    if (n != 11) {
        fprintf(stderr, "[argv-app] FAIL: toy_argv_len = %d, expected 11\n", n);
        return 1;
    }
    printf("[argv-app] PASS: toy_argv_len(argv) = %d (ptr_array marshalled)\n", n);
    return 0;
}
