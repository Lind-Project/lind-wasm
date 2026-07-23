#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <errno.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>

// Test: syscall pointer validation
// Verifies syscalls reject buffers outside the current cage

#define REGION_SIZE (64 * 1024)

int main(void)
{
	// Create an address that is no longer mapped
	unsigned char *invalid = mmap(NULL, REGION_SIZE,
	                              PROT_READ | PROT_WRITE,
	                              MAP_PRIVATE | MAP_ANONYMOUS,
	                              -1, 0);

	assert(invalid != MAP_FAILED);
	assert(munmap(invalid, REGION_SIZE) == 0);

	int fds[2];
	assert(pipe(fds) == 0);


	// write() should fail because the buffer is unmapped
	errno = 0;

	ssize_t ret = write(fds[1], invalid, REGION_SIZE);

	assert(ret == -1);
	assert(errno == EFAULT);


	// read() should also fail when writing into an invalid buffer
	unsigned char value = 0x5A;

	ret = write(fds[1], &value, 1);
	assert(ret == 1);

	errno = 0;

	ret = read(fds[0], invalid, REGION_SIZE);

	assert(ret == -1);
	assert(errno == EFAULT);


	close(fds[0]);
	close(fds[1]);

	printf("cross_cage_syscall_efault test: PASS\n");
	return 0;
}