#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
#include <errno.h>
#include <string.h>
#include <stdint.h>

static void fail(const char *msg) {
    printf("FAIL: %s\n", msg);
    exit(1);
}

int main(void) {
    long pagesz = sysconf(_SC_PAGESIZE);
    if (pagesz <= 0) {
        fail("sysconf(_SC_PAGESIZE) failed");
    }

    size_t len = (size_t)pagesz * 2;

    void *p = mmap(NULL, len, PROT_READ | PROT_WRITE,
                   MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED) {
        fail("initial mmap failed");
    }

    // Touch both pages so the mapping is definitely live.
    ((volatile char *)p)[0] = 1;
    ((volatile char *)p)[pagesz] = 2;

    // Unmap the first page.
    errno = 0;
    if (munmap(p, (size_t)pagesz) != 0) {
        fail("first munmap failed");
    }

    // The remaining mapping should now be [p + pagesz, p + 2*pagesz).
    // This is exactly the exclusive end of the remaining mapped range.
    void *edge = (char *)p + len;

    errno = 0;
    int ret = munmap(edge, (size_t)pagesz);
    int saved_errno = errno;

    // The key property under test:
    // this call must not crash/panic the runtime.
    //
    // Accept either:
    //   - success (ret == 0), or
    //   - clean failure with EINVAL
    //
    // Reject anything else.
    if (!(ret == 0 || (ret == -1 && saved_errno == EINVAL))) {
        printf("FAIL: boundary munmap returned ret=%d errno=%d (%s)\n",
               ret, saved_errno, strerror(saved_errno));
        return 1;
    }

    // Final cleanup of the second page if it is still mapped.
    errno = 0;
    ret = munmap((char *)p + pagesz, (size_t)pagesz);
    saved_errno = errno;

    // Cleanup may succeed, or may already be unmapped depending on runtime behavior above.
    // Accept either success or clean EINVAL.
    if (!(ret == 0 || (ret == -1 && saved_errno == EINVAL))) {
        printf("FAIL: cleanup munmap returned ret=%d errno=%d (%s)\n",
               ret, saved_errno, strerror(saved_errno));
        return 1;
    }

    printf("PASS\n");
    return 0;
}
