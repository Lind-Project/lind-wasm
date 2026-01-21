#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <unistd.h>
#undef NDEBUG

int main(void)
{
	const char *test_msg = "hi\n";
	const size_t test_msg_len = 3;
	char read_buf[4096] = {0};
	int ret, fd[2];

	ret = pipe(fd);
	if (ret != 0) {
		perror("pipe");
	}
	assert(ret == 0);

	ret = write(fd[1], test_msg, test_msg_len);
	if (ret < 0) {
		fprintf(stderr, "write() failed: %s\n", strerror(errno));
	}
	assert(ret == (int)test_msg_len);

	ret = read(fd[0], read_buf, test_msg_len);
	if (ret < 0) {
		fprintf(stderr, "read() failed: %s\n", strerror(errno));
	}
	assert(ret == (int)test_msg_len);

	assert(memcmp(read_buf, test_msg, test_msg_len) == 0);

	for (size_t i = 0; i < sizeof fd / sizeof *fd; i++) {
		ret = close(fd[i]);
		if (ret != 0) {
			fprintf(stderr, "close() failed: %s\n", strerror(errno));
		}
		assert(ret == 0);
	}

	return 0;
}
