#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <unistd.h>
#include <wait.h>
#include <assert.h>

// NOTE: This test assumes the test environment runs as root (UID/GID = 0).
// If tests run as non-root, these assertions will fail even if fork/exec work correctly.
#define ROOT_UID 0
#define ROOT_GID 0

int main(void)
{
	assert(getgid() == ROOT_GID && "gid should be root after exec");
	assert(getuid() == ROOT_UID && "uid should be root after exec");
	assert(getegid() == ROOT_GID && "egid should be root after exec");
	assert(geteuid() == ROOT_UID && "euid should be root after exec");

	return 0;
}