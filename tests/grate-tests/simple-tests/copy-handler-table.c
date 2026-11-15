#include <assert.h>
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

#define EXPECTED_EUID 123

int main(void) {
	int parent_euid = geteuid();
	if (parent_euid != EXPECTED_EUID) {
		fprintf(stderr,
			"[Cage|copy-handler-table] FAIL: parent expected "
			"geteuid=%d, got %d\n",
			EXPECTED_EUID, parent_euid);
		assert(0);
	}

	pid_t pid = fork();
	if (pid < 0) {
		perror("fork failed");
		assert(0);
	}

	if (pid == 0) {
		int child_euid = geteuid();
		if (child_euid != EXPECTED_EUID) {
			fprintf(stderr,
				"[Cage|copy-handler-table] FAIL: child "
				"expected inherited geteuid=%d, got %d\n",
				EXPECTED_EUID, child_euid);
			assert(0);
		}

		// Cage 1 is the grate above this cage. Copying its table over
		// our own would route around the grate, so 3i must refuse: a
		// cage may only copy tables of itself and its descendants.
		int ret = copy_handler_table_to_cage(1, getpid());
		if (ret != (int)ELINDEPERM) {
			fprintf(stderr,
				"[Cage|copy-handler-table] FAIL: copying the "
				"grate's table returned %d, expected "
				"ELINDEPERM\n",
				ret);
			assert(0);
		}

		child_euid = geteuid();
		if (child_euid != EXPECTED_EUID) {
			fprintf(stderr,
				"[Cage|copy-handler-table] FAIL: geteuid "
				"returned %d after the refused copy, expected "
				"the grate's %d\n",
				child_euid, EXPECTED_EUID);
			assert(0);
		}

		printf("[Cage|copy-handler-table] PASS: child inherited "
		       "handler and could not copy the grate's table\n");
		return 0;
	}

	int status = 0;
	pid_t waited_pid = waitpid(pid, &status, 0);
	assert(waited_pid == pid);
	assert(WIFEXITED(status));
	assert(WEXITSTATUS(status) == 0);

	printf("[Cage|copy-handler-table] PASS: parent=%d child_exit=%d\n",
	       parent_euid, WEXITSTATUS(status));
	return 0;
}
