/* A plain native program. It links libstrdemo.so and calls str_len with ordinary
 * C strings, unaware that the length is computed inside the wasm sandbox (the host
 * stub copies each string into guest memory before the call). We print libc's own
 * strlen alongside as a cross-check. */
#include <stdio.h>
#include <string.h>

/* Provided by libstrdemo.so — runs inside the lind/wasmtime sandbox. */
size_t str_len(const char *s);

int main(void) {
    const char *samples[] = {"", "hello", "lind sandbox"};
    for (int i = 0; i < 3; i++) {
        const char *s = samples[i];
        printf("str_len(\"%s\") = %zu (native strlen = %zu)\n", s, str_len(s), strlen(s));
    }
    return 0;
}
