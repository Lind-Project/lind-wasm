#include <stdio.h>
#include <ctype.h>

int main(void) {
    char tests[] = {
        'A', 'z', 'M', '9', '!', ' ', '\n', 'g', '0', 'Q'
    };
    int count = sizeof(tests) / sizeof(tests[0]);

    printf("Testing isalpha() with predefined characters:\n\n");

    for (int i = 0; i < count; i++) {
        char c = tests[i];
        if (isalpha((unsigned char)c)) {
            printf("'%c' -> isalpha: YES\n", c);
        } else {
            printf("'%c' -> isalpha: NO\n", c);
        }
    }

    return 0;
}
