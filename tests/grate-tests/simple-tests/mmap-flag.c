/* Cage side of the mmap-with-GRATE_MEMORY_FLAG test.
 *
 * fd-backed mmap → write → read → munmap round-trip.  The grate hands
 * this off to RawPOSIX with `addr_cage | GRATE_MEMORY_FLAG`, exercising
 * the runtime's flag-aware path in mmap_syscall.
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
		printf("[Cage|mmap-flag] open failed\n");
		fflush(stdout);
		return 1;
	}
	if (ftruncate(fd, size) != 0) {
		printf("[Cage|mmap-flag] ftruncate failed\n");
		fflush(stdout);
		return 1;
	}

	printf("[Cage|mmap-flag] calling fd-backed mmap (fd=%d)\n", fd);
	fflush(stdout);

	void *p = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
	if (p == MAP_FAILED) {
		printf("[Cage|mmap-flag] mmap returned MAP_FAILED\n");
		fflush(stdout);
		return 1;
	}
	printf("[Cage|mmap-flag] mmap returned %p\n", p);
	fflush(stdout);

	memset(p, 0x42, size);
	printf("[Cage|mmap-flag] memset done\n");
	fflush(stdout);

	for (size_t i = 0; i < size; i++) {
		if (((unsigned char *)p)[i] != 0x42) {
			printf("[Cage|mmap-flag] byte %zu mismatch (got 0x%x)\n",
			       i, ((unsigned char *)p)[i]);
			fflush(stdout);
			return 1;
		}
	}
	printf("[Cage|mmap-flag] readback ok\n");
	fflush(stdout);

	if (munmap(p, size) != 0) {
		printf("[Cage|mmap-flag] munmap failed\n");
		fflush(stdout);
		return 1;
	}

	close(fd);
	unlink(FILE_PATH);

	printf("[Cage|mmap-flag] PASS\n");
	fflush(stdout);
	return 0;
}
