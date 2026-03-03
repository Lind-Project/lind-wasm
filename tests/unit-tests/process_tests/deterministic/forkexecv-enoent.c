/*
 * Test to ensure that fork-exec does not panic in case the child binary doesn't
 * exist.
 */

#include <sys/types.h>
#include <unistd.h>
#include <stdio.h>
#include <sys/wait.h>

int main(void) {
	pid_t pid;

	if ((pid = fork()) == -1) {
		perror("fork error");
	} else if (pid == 0) {
		// Run a binary that doesn't exist.
		char *arr[] = {"ENOENT", NULL};
		execv("ENOENT", arr);
		perror("execv failed");
	}

	wait(NULL);
}
