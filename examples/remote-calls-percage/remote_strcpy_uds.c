#include <stdio.h>
#include <string.h>
#include <stddef.h> // for size_t

void fill_with(const char *src, char *dest, char c) {
    size_t len = 0;

    // calculate length of src
    while (src[len] != '\0') {
        len++;
    }

    // fill dest with 'a'
    for (size_t i = 0; i < len; i++) {
        dest[i] = c;
    }

    // null-terminate dest
    dest[len] = '\0';
}

char *custom_strcpy(char *dest, const char *src) {
    fill_with(src, dest, 'b');
    return dest;
}
