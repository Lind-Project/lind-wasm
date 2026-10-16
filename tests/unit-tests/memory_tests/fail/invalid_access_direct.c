/*
 * Access an invalid (unmapped) address directly.
 * Exercises lind-wasm's PROT_NONE linear memory model: pages not explicitly
 * mapped by rawposix vmmap are inaccessible, and the access should trigger a
 * wasm trap (on wasm) or SIGSEGV (on native).
 *
 * The invalid address is derived from a real, valid allocation inside this
 * cage, offset far enough to land outside the cage's mapped range. This
 * models a cross-cage wild pointer rather than an arbitrary fixed address.
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
