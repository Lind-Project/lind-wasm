#include <assert.h>
#include <errno.h>
#include <lind_syscall.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define GETPID_SYSCALL 39
#define UNUSED 0xdeadbeefdeadbeefULL

// Dispatcher every grate must export. This grate registers no handlers, so
// 3i never calls it.
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
		    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
		    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
		    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
		    uint64_t arg6, uint64_t arg6cage) {
	if (fn_ptr_uint == 0) {
		fprintf(stderr, "[Grate|grant-access] Invalid function ptr\n");
		assert(0);
	}

	int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		  uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		  uint64_t) =
	    (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		     uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		     uint64_t))(uintptr_t)fn_ptr_uint;

	return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage, arg4,
		  arg4cage, arg5, arg5cage, arg6, arg6cage);
}

// getpid made on behalf of `target`: target's cage id if this cage may act
// for it, -EPERM if 3i refuses.
static int getpid_of(uint64_t target) {
	return make_threei_call(GETPID_SYSCALL, 0, getpid(), target, UNUSED,
				UNUSED, UNUSED, UNUSED, UNUSED, UNUSED, UNUSED,
				UNUSED, UNUSED, UNUSED, UNUSED, UNUSED, 0);
}

// This grate does no interposition. It checks that it may act for the cage
// it forks, then runs the cage program, which tests grants among its own
// children.
int main(int argc, char *argv[]) {
	if (argc < 2) {
		fprintf(stderr, "Usage: %s <cage_file>\n", argv[0]);
		assert(0);
	}

	int go[2];
	if (pipe(go) < 0) {
		perror("pipe failed");
		assert(0);
	}

	pid_t pid = fork();
	if (pid < 0) {
		perror("fork failed");
		assert(0);
	} else if (pid == 0) {
		// Stay alive until the grate has acted for this cage.
		char b;
		close(go[1]);
		if (read(go[0], &b, 1) != 1) {
			perror("read failed");
			assert(0);
		}
		if (execv(argv[1], &argv[1]) == -1) {
			perror("execv failed");
			assert(0);
		}
	}

	// A cage may act for its descendants.
	int ret = getpid_of(pid);
	if (ret != pid) {
		fprintf(stderr,
			"[Grate|grant-access] FAIL: getpid on behalf of child "
			"%d returned %d\n",
			pid, ret);
		assert(0);
	}
	close(go[0]);
	if (write(go[1], "x", 1) != 1) {
		perror("write failed");
		assert(0);
	}

	int status;
	while (wait(&status) > 0) {
		if (status != 0) {
			fprintf(stderr,
				"[Grate|grant-access] FAIL: child exited with "
				"status %d\n",
				status);
			assert(0);
		}
	}

	printf("[Grate|grant-access] PASS\n");
	return 0;
}
