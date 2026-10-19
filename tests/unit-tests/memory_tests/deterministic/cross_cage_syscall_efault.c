#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>

/*
 Test: syscall pointer validation
 Checks that syscalls refuse buffers that don't belong to the current cage
*/

#define REGION_SIZE (64 * 1024)

#if defined(__wasm__)
/*
 Cage boundary cases
 A wasm32 cage only owns the first 4GB of its linear memory
 Before a syscall, glibc adds the cage's base address to the buffer pointer,
 so a buffer that runs past 4GB would have the host touching memory that
 isn't part of this cage
 Every case below should get -1 with errno set to EFAULT
 None of this means anything outside a 32-bit cage, so native skips it
*/

#define PAGE          4096u
#define CAGE_TOP_PAGE 0xFFFFF000u   // the last page before 4GB
#define CAGE_END      0x100000000ULL

static unsigned char fill[2 * PAGE];

// Non-blocking, so a really big length can't get stuck on a full pipe
static void new_pipe(int fds[2])
{
	assert(pipe2(fds, O_NONBLOCK) == 0);
}

static void close_pipe(int fds[2])
{
	close(fds[0]);
	close(fds[1]);
}

// write() should refuse the whole buffer and not send a single byte
static void expect_write_efault(uintptr_t buf, size_t len)
{
	int fds[2];
	new_pipe(fds);

	errno = 0;
	ssize_t ret = write(fds[1], (const void *)buf, len);
	assert(ret == -1);
	assert(errno == EFAULT);

	close_pipe(fds);
}

// read() should refuse the buffer even when there's data waiting in the pipe
static void expect_read_efault(uintptr_t buf, size_t len, size_t avail)
{
	int fds[2];
	new_pipe(fds);
	assert(write(fds[1], fill, avail) == (ssize_t)avail);

	errno = 0;
	ssize_t ret = read(fds[0], (void *)buf, len);
	assert(ret == -1);
	assert(errno == EFAULT);

	close_pipe(fds);
}

static void test_cage_boundary(void)
{
	/*
	 Map the last page ourselves so the part of each buffer inside the cage
	 is valid, and the only bad part is whatever sits past 4GB
	 If something else is already using that page, we leave it alone and only
	 run the write cases, since those just read from it
	*/
	int probe[2];
	new_pipe(probe);
	int top_in_use = write(probe[1], (const void *)CAGE_TOP_PAGE, 1) == 1;
	close_pipe(probe);

	if (!top_in_use) {
		void *top = mmap((void *)CAGE_TOP_PAGE, PAGE,
		                 PROT_READ | PROT_WRITE,
		                 MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED,
		                 -1, 0);
		assert(top == (void *)CAGE_TOP_PAGE);
	}

	// Case 1: buffer starts inside the cage and ends outside it
	expect_write_efault(CAGE_TOP_PAGE, 2 * PAGE);
	if (!top_in_use)
		expect_read_efault(CAGE_TOP_PAGE, 2 * PAGE, 2 * PAGE);

	// Case 2: the buffer is fine, but the length runs past 4GB
	unsigned char *valid = mmap(NULL, PAGE, PROT_READ | PROT_WRITE,
	                            MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	assert(valid != MAP_FAILED);
	uint64_t past_end = CAGE_END - (uintptr_t)valid + PAGE;
	assert(past_end <= SIZE_MAX);
	expect_write_efault((uintptr_t)valid, (size_t)past_end);
	// Only 16 bytes are in the pipe, so even if the check is missing it only writes into valid
	expect_read_efault((uintptr_t)valid, (size_t)past_end, 16);
	assert(munmap(valid, PAGE) == 0);

	/*
	 Case 3: neither the address nor the length is out of range on its own,
	 but added together they go past 4GB
	 In 32-bit math 0xFFFFFFF0 + 0x20 wraps around to 0x10
	*/
	expect_write_efault(0xFFFFFFF0u, 0x20);
	if (!top_in_use) {
		expect_read_efault(0xFFFFFFF0u, 0x20, 0x20);
		assert(munmap((void *)CAGE_TOP_PAGE, PAGE) == 0);
	}
}
#endif

int main(void)
{
	unsigned char *invalid = mmap(NULL, REGION_SIZE,
                                PROT_NONE,
                                MAP_PRIVATE | MAP_ANONYMOUS,
                                -1, 0);

	assert(invalid != MAP_FAILED);

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

#if defined(__wasm__)
	test_cage_boundary();
#endif

	printf("cross_cage_syscall_efault test: PASS\n");
	return 0;
}
