/* Cage side of the mmap-with-GRATE_MEMORY_FLAG test.
 *
 * Vanilla mmap → write → read → munmap round-trip.  Exercises the
 * mmap_syscall code path when the grate interposes and forwards with
 * `addr_cage | GRATE_MEMORY_FLAG`.
 *
 * Progress prints to stdout (not stderr) with explicit fflush so the
 * harness can see where it died if any step fails.
 */

#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

int main(void) {
	const size_t size = 4096;

	printf("[Cage|mmap-flag] calling mmap\n");
	fflush(stdout);

	void *p = mmap(NULL, size, PROT_READ | PROT_WRITE,
		       MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
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

	printf("[Cage|mmap-flag] PASS\n");
	fflush(stdout);
	return 0;
}
