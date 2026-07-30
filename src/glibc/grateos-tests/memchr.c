#include <stdio.h>
#include <string.h>

int main() {
  char str[] = "Hello, world!";
  char c = 'o';
  char *p = memchr(str, c, strlen(str));

  if (p != NULL) {
    printf("The first occurrence of '%c' in '%s' is at index %d.\n", c, str, (int)(p - str));
  } else {
    printf("The character '%c' is not found in '%s'.\n", c, str);
  }

  return 0;
}