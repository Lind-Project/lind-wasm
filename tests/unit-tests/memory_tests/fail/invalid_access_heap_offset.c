/*
 * Access an address way past the end of a valid heap allocation.
 * Exercises lind-wasm's PROT_NONE linear memory model: pages not explicitly
 * mapped by rawposix vmmap are inaccessible, and the access should trigger a
 * wasm trap (on wasm) or SIGSEGV (on native).
 *
 * The bad address starts from a real allocation inside this cage and gets
 * pushed far enough out that it lands outside the cage's mapped range. This
 * is meant to look like a wild pointer from another cage, not just some
 * random fixed address (invalid_access_direct.c covers that case).
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define BEYOND_CAGE (256u * 1024 * 1024)

int main(void)
{
	/*
	 Allocate a valid pointer inside this cage
	 Move it outside the cage memory range and verify the access traps
	*/
	volatile unsigned char *ptr =
	    (volatile unsigned char *)malloc(64);

	if (ptr == NULL)
		return 0;

	volatile unsigned char *wild =
	    (volatile unsigned char *)((uintptr_t)ptr + BEYOND_CAGE);

	// This access should trap
	*wild = 0x41;

	/*
	 Reaching here means the invalid write wasn't blocked
	 The read is included to detect possible data leakage
	 */
	unsigned char value = *wild;

	printf("LEAK: read 0x%02x outside cage memory\n", value);

	free((void *)ptr);

	return 0;
}
