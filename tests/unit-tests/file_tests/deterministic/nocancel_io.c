/*
 * Test that _nocancel I/O variants work correctly.
 *
 * The glibc "c" fopen mode flag sets _IO_FLAGS2_NOTCANCEL, which routes
 * stdio reads/writes through __read_nocancel/__write_nocancel instead of
 * __read/__write. This test verifies those paths translate pointers
 * correctly.
 *
 * Also tests getpwnam() which depends on fopen("rce") via NSS.
 */

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <pwd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(void) {
    const char *testfile = "nocancel_test.txt";
    const char *testdata = "hello from nocancel test\n";
    size_t testdata_len = strlen(testdata);

    /* ---- Test 1: fopen "wce" (write with NOTCANCEL + CLOEXEC) ---- */
    {
        FILE *f = fopen(testfile, "wce");
        assert(f != NULL);
        size_t nw = fwrite(testdata, 1, testdata_len, f);
        assert(nw == testdata_len);
        fclose(f);
    }

    /* ---- Test 2: fopen "rce" + fgets (read with NOTCANCEL + CLOEXEC) ---- */
    {
        FILE *f = fopen(testfile, "rce");
        assert(f != NULL);
        char buf[128] = {0};
        char *ret = fgets(buf, sizeof(buf), f);
        assert(ret != NULL);
        assert(strcmp(buf, testdata) == 0);
        fclose(f);
    }

    /* ---- Test 3: fopen "rce" + getline (malloc'd buffer + NOTCANCEL read) ---- */
    {
        FILE *f = fopen(testfile, "rce");
        assert(f != NULL);
        char *line = NULL;
        size_t len = 0;
        ssize_t nread = getline(&line, &len, f);
        assert(nread == (ssize_t)testdata_len);
        assert(strcmp(line, testdata) == 0);
        free(line);
        fclose(f);
    }

    /* ---- Test 4: fopen "rce" + fseeko + getline (NSS-like pattern) ---- */
    {
        FILE *f = fopen(testfile, "rce");
        assert(f != NULL);
        int seekret = fseeko(f, 0, SEEK_SET);
        assert(seekret == 0);
        char *line = NULL;
        size_t len = 0;
        ssize_t nread = getline(&line, &len, f);
        assert(nread == (ssize_t)testdata_len);
        assert(strcmp(line, testdata) == 0);
        free(line);
        fclose(f);
    }

    /* ---- Test 5: getpwnam (requires /etc/passwd + /etc/nsswitch.conf) ---- */
    {
        /* First verify the config files exist */
        FILE *f = fopen("/etc/passwd", "r");
        if (f != NULL) {
            fclose(f);
            f = fopen("/etc/nsswitch.conf", "r");
            if (f != NULL) {
                fclose(f);
                errno = 0;
                struct passwd *pw = getpwnam("root");
                assert(pw != NULL);
                assert(pw->pw_uid == 0);
                assert(pw->pw_gid == 0);
            }
        }
    }

    /* Clean up */
    unlink(testfile);

    return 0;
}
