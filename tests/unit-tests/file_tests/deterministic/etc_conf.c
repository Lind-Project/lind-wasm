#include <assert.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    char buf[256];

    /* /etc/passwd exists and has at least one line */
    FILE *f = fopen("/etc/passwd", "r");
    assert(f != NULL);
    char *ret = fgets(buf, sizeof(buf), f);
    assert(ret != NULL);
    assert(strlen(buf) > 0);
    /* basic sanity: passwd lines contain colons */
    assert(strchr(buf, ':') != NULL);
    fclose(f);

    /* /etc/nsswitch.conf exists and has at least one non-empty line */
    f = fopen("/etc/nsswitch.conf", "r");
    assert(f != NULL);
    ret = fgets(buf, sizeof(buf), f);
    assert(ret != NULL);
    assert(strlen(buf) > 0);
    fclose(f);

    return 0;
}
