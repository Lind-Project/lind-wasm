/* Cage side of the mmap-with-GRATE_MEMORY_FLAG test.
 *
 * This is a vanilla mmap → write → read → munmap round-trip.  Its job is
 * to exercise the mmap_syscall code path when the grate interposes and
 * forwards the call with `addr_cageid | GRATE_MEMORY_FLAG`.  Before the
 * runtime patch this trigggered a 32-bit-truncation bug in mmap_syscall
 * that landed the mapping at an arbitrary cage address and clobbered the
 * cage's stack — manifesting as e.g. an EBADF on a later syscall using a
 * stack-resident fd whose value got memcpy'd over.
 *
 * If the runtime handles the flag correctly, write-then-readback in the
 * mapped region preserves the data and munmap succeeds.
 */

#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

int main(void) {
	const size_t size = 4096;
	void *p = mmap(NULL, size, PROT_READ | PROT_WRITE,
		       MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
	if (p == MAP_FAILED) {
		perror("mmap");
		return 1;
	}

	memset(p, 0x42, size);
	for (size_t i = 0; i < size; i++) {
		if (((unsigned char *)p)[i] != 0x42) {
			fprintf(stderr, "byte %zu mismatch (got 0x%x)\n", i,
				((unsigned char *)p)[i]);
			return 1;
		}
	}

	if (munmap(p, size) != 0) {
		perror("munmap");
		return 1;
	}

	printf("[Cage|mmap-flag] PASS\n");
	return 0;
}
