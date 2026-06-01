#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <assert.h>

int main(void) {
    /* Test 1: getcwd with provided buffer */
    char buf[1024];
    char *ret = getcwd(buf, sizeof(buf));
    assert(ret == buf);
    assert(strlen(buf) > 0);
    assert(buf[0] == '/');

    /* Test 2: getcwd(NULL, 0) — glibc should allocate */
    char *allocated = getcwd(NULL, 0);
    assert(allocated != NULL);
    assert(strlen(allocated) > 0);
    assert(allocated[0] == '/');
    assert(strcmp(allocated, buf) == 0);
    free(allocated);

    /* Test 3: getcwd(NULL, size) — glibc should allocate with given size */
    char *allocated2 = getcwd(NULL, 1024);
    assert(allocated2 != NULL);
    assert(strcmp(allocated2, buf) == 0);
    free(allocated2);

    printf("getcwd_null: all tests passed\n");
    return 0;
}
