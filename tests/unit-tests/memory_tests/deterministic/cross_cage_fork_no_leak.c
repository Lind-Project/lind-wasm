#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

/*
 Test: fork memory isolation
 Checks that a parent and its forked child never end up sharing writable memory
*/

#define PARENT_SENTINEL 0xA1
#define CHILD_SENTINEL  0xB2

#define BUF_SIZE (256 * 1024)

static void wait_child(pid_t pid)
{
	int status;
	pid_t ret = waitpid(pid, &status, 0);

	assert(ret >= 0);
	assert(WIFEXITED(status));
	assert(WEXITSTATUS(status) == 0);
}

static void assert_all(const unsigned char *buf, size_t size,
                       unsigned char value)
{
	for (size_t i = 0; i < size; i++)
		assert(buf[i] == value);
}

int main(void)
{
	pid_t pid;

	// Test 1: Heap memory
	unsigned char *heap = malloc(BUF_SIZE);
	assert(heap != NULL);

	memset(heap, PARENT_SENTINEL, BUF_SIZE);

	pid = fork();
	assert(pid >= 0);

	if (pid == 0) {
		// Child sees initial contents
		assert_all(heap, BUF_SIZE, PARENT_SENTINEL);

		// Child writes should not affect parent memory
		memset(heap, CHILD_SENTINEL, BUF_SIZE);
		assert_all(heap, BUF_SIZE, CHILD_SENTINEL);

		exit(0);
	}

	wait_child(pid);

	// Parent should still see its original data
	assert_all(heap, BUF_SIZE, PARENT_SENTINEL);

	free(heap);


	// Test 2: anonymous mmap 
	unsigned char *mapped = mmap(NULL, BUF_SIZE,
	                             PROT_READ | PROT_WRITE,
	                             MAP_PRIVATE | MAP_ANONYMOUS,
	                             -1, 0);
	assert(mapped != MAP_FAILED);

	memset(mapped, PARENT_SENTINEL, BUF_SIZE);

	pid = fork();
	assert(pid >= 0);

	if (pid == 0) {
		assert_all(mapped, BUF_SIZE, PARENT_SENTINEL);

		memset(mapped, CHILD_SENTINEL, BUF_SIZE);
		assert_all(mapped, BUF_SIZE, CHILD_SENTINEL);

		exit(0);
	}

	wait_child(pid);

	assert_all(mapped, BUF_SIZE, PARENT_SENTINEL);

	assert(munmap(mapped, BUF_SIZE) == 0);


	// Test 3: allocation after fork 
	unsigned char *before = malloc(BUF_SIZE);
	assert(before != NULL);

	memset(before, PARENT_SENTINEL, BUF_SIZE);

	pid = fork();
	assert(pid >= 0);

	if (pid == 0) {
		// Child scribbles over the buffer it inherited, the parent shouldn't see this
		memset(before, CHILD_SENTINEL, BUF_SIZE);
		assert_all(before, BUF_SIZE, CHILD_SENTINEL);

		unsigned char *child_buf = malloc(BUF_SIZE);
		assert(child_buf != NULL);

		memset(child_buf, CHILD_SENTINEL, BUF_SIZE);
		assert_all(child_buf, BUF_SIZE, CHILD_SENTINEL);

		free(child_buf);
		exit(0);
	}

	wait_child(pid);

	assert_all(before, BUF_SIZE, PARENT_SENTINEL);

	free(before);


	// Test 4: same virtual address mapped separately
	void *slot = mmap(NULL, BUF_SIZE,
	                  PROT_READ | PROT_WRITE,
	                  MAP_PRIVATE | MAP_ANONYMOUS,
	                  -1, 0);
	assert(slot != MAP_FAILED);

	assert(munmap(slot, BUF_SIZE) == 0);

	/*
	 Use two pipes so things happen in a fixed order
	 The child writes first, then the parent checks its copy and writes,
	 then the child checks its copy again
	 That way each side gets to confirm the other's write never reached it
	*/
	int child_ready[2], parent_done[2];
	char token;
	assert(pipe(child_ready) == 0);
	assert(pipe(parent_done) == 0);

	pid = fork();
	assert(pid >= 0);

	if (pid == 0) {
		close(child_ready[0]);
		close(parent_done[1]);

		unsigned char *child_map = mmap(slot, BUF_SIZE,
		                               PROT_READ | PROT_WRITE,
		                               MAP_PRIVATE | MAP_ANONYMOUS |
		                               MAP_FIXED,
		                               -1, 0);

		assert(child_map == slot);

		memset(child_map, CHILD_SENTINEL, BUF_SIZE);

		// Let the parent know we're done writing, then wait for it to write too
		assert(write(child_ready[1], "c", 1) == 1);
		assert(read(parent_done[0], &token, 1) == 1);

		// The parent has written by now, our copy should still be untouched
		assert_all(child_map, BUF_SIZE, CHILD_SENTINEL);

		exit(0);
	}

	close(child_ready[1]);
	close(parent_done[0]);

	unsigned char *parent_map = mmap(slot, BUF_SIZE,
	                                 PROT_READ | PROT_WRITE,
	                                 MAP_PRIVATE | MAP_ANONYMOUS |
	                                 MAP_FIXED,
	                                 -1, 0);

	assert(parent_map == slot);

	/*
	 Wait for the child to finish writing, then make sure none of it leaked over
	 A brand new anonymous mapping starts out as all zeros
	*/
	assert(read(child_ready[0], &token, 1) == 1);
	assert_all(parent_map, BUF_SIZE, 0);

	memset(parent_map, PARENT_SENTINEL, BUF_SIZE);
	assert(write(parent_done[1], "p", 1) == 1);

	wait_child(pid);

	close(child_ready[0]);
	close(parent_done[1]);

	// Same address in both cages, but the data shouldn't be shared
	assert_all(parent_map, BUF_SIZE, PARENT_SENTINEL);

	assert(munmap(parent_map, BUF_SIZE) == 0);


	printf("cross_cage_fork_no_leak test: PASS\n");
	return 0;
}
