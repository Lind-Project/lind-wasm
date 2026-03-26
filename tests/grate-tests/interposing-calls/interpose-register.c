#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <assert.h>
#include <lind_syscall.h>
#include <sys/wait.h>

// This is grate-2. It spawns a child cage and tries to interpose the child's
// geteuid syscall.
int main(int argc, char *argv[]) {
	int grateid = getpid();

	int pid = fork();

	if (pid < 0) {
		exit(1);
	} else if (pid == 0) {
		int cageid = getpid();
		// This grateid has it's register_handler interposed. This call
		// should go to the grate-1.
		printf("[cage] registering 107. grateid: %d cageid: %d\n",
		       grateid, cageid);
		register_handler(cageid, 107, grateid, 0);

		int ret = geteuid();
		if (ret != 10) {
			assert(0);
		}
	} else {
		wait(NULL);
	}

	return 0;
}
