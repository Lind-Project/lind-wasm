// Measurement cage: exercises pure libc string functions through the
// auto-interposition grate. All 7 are marshalled (interposed → grate's libc).
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

static int pass = 0, fail = 0;
static void check(const char *name, int ok, const char *detail) {
    if (ok) { printf("  PASS  %s  %s\n", name, detail); pass++; }
    else    { printf("  FAIL  %s  %s\n", name, detail); fail++; }
}

// data-segment buffers (cross-cage reachable)
static const char hello[]  = "hello world";
static const char abc[]    = "abc";
static const char abd[]    = "abd";
static const char numstr[] = "12345abc";

int main(void) {
    char buf[64];

    // strlen — cstr sizing
    size_t L = strlen(hello);
    snprintf(buf, sizeof buf, "= %zu (want 11)", L);
    check("strlen", L == 11, buf);

    // strnlen — from_arg(1)
    size_t L2 = strnlen(hello, 5);
    snprintf(buf, sizeof buf, "= %zu (want 5)", L2);
    check("strnlen", L2 == 5, buf);

    // memcmp — two in-buffers sized by arg2
    int e = memcmp(abc, abc, 3);
    int g = memcmp(abc, abd, 3);
    snprintf(buf, sizeof buf, "eq=%d lt=%d", e, g);
    check("memcmp", e == 0 && g < 0, buf);

    // strcmp — two cstrings
    int se = strcmp(abc, abc);
    int sg = strcmp(abc, abd);
    snprintf(buf, sizeof buf, "eq=%d lt=%d", se, sg);
    check("strcmp", se == 0 && sg < 0, buf);

    // strncmp — from_arg(2)
    int n = strncmp("abcXY", "abcZ", 3);
    snprintf(buf, sizeof buf, "= %d (want 0)", n);
    check("strncmp", n == 0, buf);

    // memchr — ret ptr_into_arg: must return a pointer INTO hello (offset 6 = 'w')
    void *p = memchr(hello, 'w', sizeof(hello));
    long off = p ? (long)((const char *)p - hello) : -1;
    snprintf(buf, sizeof buf, "off=%ld (want 6, ptr-into-arg)", off);
    check("memchr", off == 6, buf);

    // strchr — ret ptr_into_arg: 'o' at offset 4
    char *q = strchr(hello, 'o');
    long off2 = q ? (long)(q - hello) : -1;
    snprintf(buf, sizeof buf, "off=%ld (want 4, ptr-into-arg)", off2);
    check("strchr", off2 == 4, buf);

    // strtol — char** endptr: out pointer-to-pointer aliasing arg0.
    // *end must point into numstr at offset 5 ("abc"); value 12345.
    char *end = NULL;
    long v = strtol(numstr, &end, 10);
    long endoff = end ? (long)(end - numstr) : -1;
    snprintf(buf, sizeof buf, "v=%ld endoff=%ld (want 12345, 5)", v, endoff);
    check("strtol(endptr)", v == 12345 && endoff == 5, buf);

    printf("[libc-app] %d passed, %d failed\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
