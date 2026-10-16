#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

// Test: wild pointer isolation
// Verifies accesses outside cage memory are trapped

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
		return 1;


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