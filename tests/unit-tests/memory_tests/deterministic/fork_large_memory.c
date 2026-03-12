#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <unistd.h>

/*
 * Reproduce issue #813: memory fault after fork when child accesses
 * memory at higher addresses.
 *
 * Test 1: Large heap — push program_break high, fork, child reads back
 * Test 2: mmap region — anonymous mmap, fork, child reads back
 * Test 3: Nested fork — grow heap, fork, grow more, fork again
 * Test 4: Guard page pattern — mmap PROT_NONE + mprotect partial RW, fork
 * Test 5: Fragmented mmaps — many small mmaps creating lots of vmmap entries
 * Test 6: Network sockets before fork — mimics lmbench lat_tcp
 * Test 7: mmap + munmap holes — create gaps in vmmap, fork
 */

#define PATTERN(i) ((unsigned char)(0xA0 + ((i) & 0x3F)))

static void wait_child(pid_t pid)
{
	int status;
	pid_t w = waitpid(pid, &status, 0);
	assert(w >= 0);
	assert(WIFEXITED(status));
	if (WEXITSTATUS(status) != 0) {
		printf("FAIL: child exited with status %d\n", WEXITSTATUS(status));
	}
	assert(WEXITSTATUS(status) == 0);
}

int main(void)
{
	pid_t pid;
	int status;

	/* ---- Test 1: Large heap ---- */
	#define NCHUNKS 8
	#define CHUNK_SIZE (2 * 1024 * 1024)

	char *chunks[NCHUNKS];
	for (int i = 0; i < NCHUNKS; i++) {
		chunks[i] = (char *)malloc(CHUNK_SIZE);
		assert(chunks[i] != NULL);
		memset(chunks[i], PATTERN(i), CHUNK_SIZE);
	}

	pid = fork();
	assert(pid >= 0);
	if (pid == 0) {
		for (int i = 0; i < NCHUNKS; i++) {
			unsigned char pat = PATTERN(i);
			assert((unsigned char)chunks[i][0] == pat);
			assert((unsigned char)chunks[i][CHUNK_SIZE - 1] == pat);
			for (int off = 0; off < CHUNK_SIZE; off += 4096)
				assert((unsigned char)chunks[i][off] == pat);
		}
		char *child_buf = (char *)malloc(1024 * 1024);
		assert(child_buf != NULL);
		memset(child_buf, 0xCC, 1024 * 1024);
		free(child_buf);
		exit(0);
	}
	wait_child(pid);
	for (int i = 0; i < NCHUNKS; i++) free(chunks[i]);
	printf("Test 1 PASS: large heap survives fork\n");

	/* ---- Test 2: mmap region ---- */
	#define MMAP_SIZE (4 * 1024 * 1024)

	char *mapped = (char *)mmap(NULL, MMAP_SIZE,
	                            PROT_READ | PROT_WRITE,
	                            MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	assert(mapped != MAP_FAILED);
	memset(mapped, 0xBE, MMAP_SIZE);

	pid = fork();
	assert(pid >= 0);
	if (pid == 0) {
		assert((unsigned char)mapped[0] == 0xBE);
		assert((unsigned char)mapped[MMAP_SIZE - 1] == 0xBE);
		for (int off = 0; off < MMAP_SIZE; off += 4096)
			assert((unsigned char)mapped[off] == 0xBE);
		exit(0);
	}
	wait_child(pid);
	munmap(mapped, MMAP_SIZE);
	printf("Test 2 PASS: mmap region survives fork\n");

	/* ---- Test 3: Nested fork with heap growth ---- */
	char *pre = (char *)malloc(4 * 1024 * 1024);
	assert(pre != NULL);
	memset(pre, 0xAA, 4 * 1024 * 1024);

	pid = fork();
	assert(pid >= 0);
	if (pid == 0) {
		char *extra = (char *)malloc(4 * 1024 * 1024);
		assert(extra != NULL);
		memset(extra, 0xBB, 4 * 1024 * 1024);

		pid_t pid2 = fork();
		assert(pid2 >= 0);
		if (pid2 == 0) {
			assert((unsigned char)pre[0] == 0xAA);
			assert((unsigned char)pre[4 * 1024 * 1024 - 1] == 0xAA);
			assert((unsigned char)extra[0] == 0xBB);
			assert((unsigned char)extra[4 * 1024 * 1024 - 1] == 0xBB);
			exit(0);
		}
		wait_child(pid2);
		free(extra);
		exit(0);
	}
	wait_child(pid);
	free(pre);
	printf("Test 3 PASS: nested fork with heap growth\n");

	/* ---- Test 4: Guard page pattern (mmap PROT_NONE + partial mprotect) ---- */
	#define GUARD_TOTAL (16 * 4096)  /* 64 KB total */
	#define GUARD_SIZE  4096         /* 4 KB guard at start */

	char *guarded = (char *)mmap(NULL, GUARD_TOTAL,
	                             PROT_NONE,
	                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	assert(guarded != MAP_FAILED);

	/* Make everything after the guard page RW */
	int ret = mprotect(guarded + GUARD_SIZE, GUARD_TOTAL - GUARD_SIZE,
	                   PROT_READ | PROT_WRITE);
	assert(ret == 0);

	/* Fill the accessible part */
	memset(guarded + GUARD_SIZE, 0xDD, GUARD_TOTAL - GUARD_SIZE);

	pid = fork();
	assert(pid >= 0);
	if (pid == 0) {
		/* Child: accessible part should have our pattern */
		assert((unsigned char)guarded[GUARD_SIZE] == 0xDD);
		assert((unsigned char)guarded[GUARD_TOTAL - 1] == 0xDD);
		for (int off = GUARD_SIZE; off < GUARD_TOTAL; off += 4096)
			assert((unsigned char)guarded[off] == 0xDD);
		exit(0);
	}
	wait_child(pid);
	munmap(guarded, GUARD_TOTAL);
	printf("Test 4 PASS: guard page + mprotect survives fork\n");

	/* ---- Test 5: Many fragmented mmaps ---- */
	#define FRAG_COUNT 64
	#define FRAG_SIZE  (16 * 1024)  /* 16 KB each */

	char *frags[FRAG_COUNT];
	for (int i = 0; i < FRAG_COUNT; i++) {
		frags[i] = (char *)mmap(NULL, FRAG_SIZE,
		                        PROT_READ | PROT_WRITE,
		                        MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
		assert(frags[i] != MAP_FAILED);
		memset(frags[i], PATTERN(i), FRAG_SIZE);
	}

	pid = fork();
	assert(pid >= 0);
	if (pid == 0) {
		for (int i = 0; i < FRAG_COUNT; i++) {
			unsigned char pat = PATTERN(i);
			assert((unsigned char)frags[i][0] == pat);
			assert((unsigned char)frags[i][FRAG_SIZE - 1] == pat);
		}
		exit(0);
	}
	wait_child(pid);
	for (int i = 0; i < FRAG_COUNT; i++) munmap(frags[i], FRAG_SIZE);
	printf("Test 5 PASS: fragmented mmaps survive fork\n");

	/* ---- Test 6: Network sockets before fork (lmbench pattern) ---- */
	int sockfd = socket(AF_INET, SOCK_STREAM, 0);
	assert(sockfd >= 0);

	int opt = 1;
	setsockopt(sockfd, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));

	struct sockaddr_in addr;
	memset(&addr, 0, sizeof(addr));
	addr.sin_family = AF_INET;
	addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
	addr.sin_port = htons(0);  /* kernel picks port */

	ret = bind(sockfd, (struct sockaddr *)&addr, sizeof(addr));
	assert(ret == 0);
	ret = listen(sockfd, 5);
	assert(ret == 0);

	/* Allocate a big buffer like lmbench does */
	#define NET_BUF_SIZE (8 * 1024 * 1024)
	char *netbuf = (char *)malloc(NET_BUF_SIZE);
	assert(netbuf != NULL);
	memset(netbuf, 0xEE, NET_BUF_SIZE);

	pid = fork();
	assert(pid >= 0);
	if (pid == 0) {
		/* Child: verify buffer and socket are accessible */
		assert((unsigned char)netbuf[0] == 0xEE);
		assert((unsigned char)netbuf[NET_BUF_SIZE - 1] == 0xEE);
		for (int off = 0; off < NET_BUF_SIZE; off += 4096)
			assert((unsigned char)netbuf[off] == 0xEE);
		close(sockfd);
		exit(0);
	}
	wait_child(pid);
	close(sockfd);
	free(netbuf);
	printf("Test 6 PASS: socket + large buffer survives fork\n");

	/* ---- Test 7: mmap + munmap holes ---- */
	#define HOLE_SIZE (8 * 4096)  /* 32 KB */
	char *a = mmap(NULL, HOLE_SIZE, PROT_READ | PROT_WRITE,
	               MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	char *b = mmap(NULL, HOLE_SIZE, PROT_READ | PROT_WRITE,
	               MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	char *c = mmap(NULL, HOLE_SIZE, PROT_READ | PROT_WRITE,
	               MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	assert(a != MAP_FAILED && b != MAP_FAILED && c != MAP_FAILED);

	memset(a, 0x11, HOLE_SIZE);
	memset(b, 0x22, HOLE_SIZE);
	memset(c, 0x33, HOLE_SIZE);

	/* Punch a hole by unmapping the middle one */
	munmap(b, HOLE_SIZE);

	pid = fork();
	assert(pid >= 0);
	if (pid == 0) {
		/* Child: a and c should be intact */
		assert((unsigned char)a[0] == 0x11);
		assert((unsigned char)a[HOLE_SIZE - 1] == 0x11);
		assert((unsigned char)c[0] == 0x33);
		assert((unsigned char)c[HOLE_SIZE - 1] == 0x33);
		/* Don't touch b — it's unmapped */
		exit(0);
	}
	wait_child(pid);
	munmap(a, HOLE_SIZE);
	munmap(c, HOLE_SIZE);
	printf("Test 7 PASS: mmap/munmap holes survive fork\n");

	printf("All tests passed.\n");
	return 0;
}
