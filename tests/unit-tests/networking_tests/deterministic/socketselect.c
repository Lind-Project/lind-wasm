#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <unistd.h>

#define MSG "PING"

int main(void)
{
	int sv[2];
	int ret = socketpair(AF_UNIX, SOCK_STREAM, 0, sv);
	assert(ret == 0);

	fd_set readfds;
	FD_ZERO(&readfds);
	FD_SET(sv[1], &readfds);

	size_t len = strlen(MSG);
	ssize_t n = write(sv[0], MSG, len);
	assert(n == (ssize_t)len);

	ret = select(sv[1] + 1, &readfds, NULL, NULL, NULL);
	assert(ret == 1);
	assert(FD_ISSET(sv[1], &readfds));

	char buf[32];
	size_t total = 0;
	while (total < len) {
		n = read(sv[1], buf + total, len - total);
		assert(n > 0);
		total += (size_t)n;
	}
	assert(memcmp(buf, MSG, len) == 0);

	close(sv[0]);
	close(sv[1]);
	return 0;
}
