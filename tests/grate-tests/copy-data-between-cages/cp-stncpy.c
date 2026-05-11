#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <errno.h>

int main() {
    long page_size = sysconf(_SC_PAGESIZE);
    if (page_size <= 0) {
        perror("sysconf(_SC_PAGESIZE) failed");
        return 1;
    }

    /*
     * Allocate two contiguous pages:
     *
     *   [ page 0: readable/writable ][ page 1: inaccessible ]
     *
     * We place "random\0" at the very end of page 0, so the byte
     * immediately after '\0' belongs to page 1 and is invalid.
     */
    char *region = mmap(
        NULL,
        page_size * 2,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS,
        -1,
        0
    );

    if (region == MAP_FAILED) {
        perror("mmap failed");
        return 1;
    }

    if (mprotect(region + page_size, page_size, PROT_NONE) != 0) {
        perror("mprotect failed");
        munmap(region, page_size * 2);
        return 1;
    }

    const char *name = "random";
    size_t name_len = strlen(name) + 1;  // include '\0'

    /*
     * Put "random\0" at the last 7 bytes of the first page.
     *
     * After this:
     *
     *   pathname[0..5] = "random"
     *   pathname[6]    = '\0'
     *   pathname[7]    = invalid memory
     */
    char *pathname = region + page_size - name_len;
    memcpy(pathname, name, name_len);

    printf("[cage] pathname addr=%p\n", (void *)pathname);
    printf("[cage] pathname='%s'\n", pathname);

    int fd = open(pathname, O_CREAT | O_RDONLY, 0544);

    /*
     * Expected behavior:
     *
     * - If 3i copies pathname using bounded strncpy semantics,
     *   this should succeed and return the arbitrary value from grate,
     *   e.g. 10.
     *
     * - If 3i tries to memcpy 4096 bytes from pathname,
     *   it should fail because bytes after "random\0" are inaccessible.
     */
    printf("[cage] fd=%d\n", fd);

    munmap(region, page_size * 2);
    return 0;
}
