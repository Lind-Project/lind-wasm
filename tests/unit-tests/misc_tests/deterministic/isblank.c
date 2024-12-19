#include <stdio.h>
#include <ctype.h>

int main() {
    const char *str = "Hello\t World!";  // Example string with a tab and a space
    int i = 0;

    printf("Checking for blank spaces in the string: \"%s\"\n", str);

    while (str[i] != '\0') {  // Loop through the string until the null terminator
        if (isblank(str[i])) {
            printf("Character at position %d is a blank space: '%c'\n", i, str[i]);
        }
        i++;
    }

    return 0;
}
