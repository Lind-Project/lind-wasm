#include <assert.h>
#include <errno.h>
#include <lind_syscall.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

#define GETPID_SYSCALL 39
#define UNUSED 0xdeadbeefdeadbeefULL

#define CHECK(cond, ...)                                                       \
	do {                                                                   \
		if (!(cond)) {                                                 \
			fprintf(stderr, "[Cage|grant-access] FAIL: ");         \
			fprintf(stderr, __VA_ARGS__);                          \
			fprintf(stderr, "\n");                                 \
			assert(0);                                             \
		}                                                              \
	} while (0)

// getpid made on behalf of `target`: target's cage id if this cage may act
// for it, -EPERM if 3i refuses.
static int getpid_of(uint64_t target) {
	return make_threei_call(GETPID_SYSCALL, 0, getpid(), target, UNUSED,
				UNUSED, UNUSED, UNUSED, UNUSED, UNUSED, UNUSED,
				UNUSED, UNUSED, UNUSED, UNUSED, UNUSED, 0);
}

static void wait_byte(int fd) {
	char b;
	CHECK(read(fd, &b, 1) == 1, "read from pipe failed");
}

static void send_byte(int fd) {
	CHECK(write(fd, "x", 1) == 1, "write to pipe failed");
}

// This cage forks two children, s1 and s2. Siblings may not act for each
// other until their parent grants it, and not after it revokes the grant.
int main(void) {
	int ret;

	// A cage may not act for the grate above it.
	ret = getpid_of(getppid());
	CHECK(ret == -EPERM, "getpid on behalf of parent returned %d", ret);

	int hold[2], to_parent[2], to_s2[2];
	CHECK(pipe(hold) == 0 && pipe(to_parent) == 0 && pipe(to_s2) == 0,
	      "pipe failed");

	pid_t s1 = fork();
	CHECK(s1 >= 0, "fork s1 failed");
	if (s1 == 0) {
		// s1 only has to exist until the parent releases it.
		wait_byte(hold[0]);
		_exit(0);
	}

	// A cage may act for its own child.
	ret = getpid_of(s1);
	CHECK(ret == s1, "getpid on behalf of child %d returned %d", s1, ret);

	pid_t s2 = fork();
	CHECK(s2 >= 0, "fork s2 failed");
	if (s2 == 0) {
		ret = getpid_of(s1);
		CHECK(ret == -EPERM,
		      "s2 acted for sibling %d before a grant (ret %d)", s1,
		      ret);

		// s2 does not own s1, so it cannot grant itself access.
		ret = grant_cage_access(s1, getpid());
		CHECK(ret == (int)ELINDEPERM,
		      "s2 granted itself access to s1 (ret %d)", ret);

		send_byte(to_parent[1]);
		wait_byte(to_s2[0]); // parent granted s2 access to s1

		ret = getpid_of(s1);
		CHECK(ret == s1, "s2 could not act for s1 after the grant (ret %d)",
		      ret);

		send_byte(to_parent[1]);
		wait_byte(to_s2[0]); // parent revoked it

		ret = getpid_of(s1);
		CHECK(ret == -EPERM,
		      "s2 still acted for s1 after the revoke (ret %d)", ret);
		_exit(0);
	}

	wait_byte(to_parent[0]);
	ret = grant_cage_access(s1, s2);
	CHECK(ret == 0, "grant_cage_access(s1, s2) returned %d", ret);
	send_byte(to_s2[1]);

	wait_byte(to_parent[0]);
	ret = revoke_cage_access(s1, s2);
	CHECK(ret == 0, "revoke_cage_access(s1, s2) returned %d", ret);
	send_byte(to_s2[1]);

	int status = 0;
	CHECK(waitpid(s2, &status, 0) == s2, "waitpid s2 failed");
	CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0,
	      "s2 exited with status %d", status);

	send_byte(hold[1]);
	CHECK(waitpid(s1, &status, 0) == s1, "waitpid s1 failed");
	CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0,
	      "s1 exited with status %d", status);

	printf("[Cage|grant-access] PASS\n");
	return 0;
}
