// Test that TLS-dependent ctype/stdlib functions work correctly in forked
// children. Regression test for a bug where __tls_base (WASM global[1]) was
// not restored after fork, causing all TLS accesses (__ctype_b, locale data,
// etc.) to hit wrong addresses and trigger spurious memory faults.
//
// The original failure was: gethostbyname -> inet_aton -> strtoul -> isspace
// faulting in the child because __ctype_b pointed to garbage.

#include <assert.h>
#include <ctype.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

// Exercise ctype classification (uses __ctype_b TLS pointer)
static void check_ctype(void) {
    assert(isspace(' '));
    assert(isspace('\t'));
    assert(isspace('\n'));
    assert(!isspace('A'));
    assert(!isspace('0'));

    assert(isdigit('0'));
    assert(isdigit('9'));
    assert(!isdigit('A'));

    assert(isalpha('a'));
    assert(isalpha('Z'));
    assert(!isalpha('5'));

    assert(isupper('A'));
    assert(!isupper('a'));
    assert(islower('z'));
    assert(!islower('Z'));
}

// Exercise strtoul/strtol (uses ctype + locale TLS data internally)
static void check_strtol(void) {
    char *end;

    long v1 = strtol("12345", &end, 10);
    assert(v1 == 12345);
    assert(*end == '\0');

    unsigned long v2 = strtoul("0xDEAD", &end, 16);
    assert(v2 == 0xDEAD);
    assert(*end == '\0');

    long v3 = strtol("  -42  ", &end, 10);
    assert(v3 == -42);

    // Base detection with leading 0
    long v4 = strtol("0777", &end, 0);
    assert(v4 == 0777);
}

int main() {
    // Verify parent works first
    check_ctype();
    check_strtol();

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        // Child: these would fault with __tls_base=0
        check_ctype();
        check_strtol();
        _exit(0);
    } else {
        int status;
        pid_t waited = waitpid(pid, &status, 0);
        assert(waited == pid);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);

        // Parent still works after child ran
        check_ctype();
        check_strtol();
    }

    return 0;
}
