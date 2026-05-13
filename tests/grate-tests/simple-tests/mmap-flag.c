/* Cage side of the mmap-with-GRATE_MEMORY_FLAG test.
 *
 * fd-backed mmap → write → read → munmap round-trip.  The companion
 * grate forwards this call to RawPOSIX with `arg1cage | GRATE_MEMORY_FLAG`,
 * exercising the runtime's flag-aware path in mmap_syscall.
 *
 * Anonymous mmaps (including the runtime's own pre-main stack setup)
 * are forwarded by the grate without the flag and aren't exercised here.
 */

#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

#define FILE_PATH "mmap-flag.tmp"

int main(void) {
	const size_t size = 4096;

	int fd = open(FILE_PATH, O_RDWR | O_CREAT | O_TRUNC, 0666);
	if (fd < 0) {
		return 1;
	}
	if (ftruncate(fd, size) != 0) {
		return 1;
	}

	void *p = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
	if (p == MAP_FAILED) {
		return 1;
	}

	memset(p, 0x42, size);
	for (size_t i = 0; i < size; i++) {
		if (((unsigned char *)p)[i] != 0x42) {
			return 1;
		}
	}

	if (munmap(p, size) != 0) {
		return 1;
	}

	close(fd);
	unlink(FILE_PATH);

	printf("[Cage|mmap-flag] PASS\n");
	return 0;
}
