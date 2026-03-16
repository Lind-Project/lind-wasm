/*
 * Deterministic test of shutdown() using socketpair.
 * Case: SHUT_WR on sv[0] -> write(sv[0]) fails with EPIPE; read(sv[1]) returns 0 after drain (EOF).
 */

#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>

#define BUF_SIZE 64

static void fail(const char *msg)
{
	fprintf(stderr, "%s\n", msg);
	exit(EXIT_FAILURE);
}

int main(void)
{
	int sv[2];
	char buf[BUF_SIZE];
	ssize_t n;

	signal(SIGPIPE, SIG_IGN);  /* so write() returns -1/EPIPE instead of killing process */

	if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0)
		fail("socketpair failed");

	/* Write one byte so peer has something to read before EOF */
	if (write(sv[0], "x", 1) != 1)
		fail("write before shutdown failed");

	if (shutdown(sv[0], SHUT_WR) != 0)
		fail("shutdown SHUT_WR failed");

	/* Writing after SHUT_WR must fail with EPIPE (or ECONNRESET on some platforms) */
	n = write(sv[0], "y", 1);
	if (n != -1)
		fail("write after SHUT_WR should fail");
	if (errno != EPIPE && errno != ECONNRESET)
		fail("write after SHUT_WR: expected EPIPE or ECONNRESET");

	/* Drain sv[1]: read the byte we wrote, then EOF */
	n = read(sv[1], buf, sizeof(buf));
	if (n != 1 || buf[0] != 'x')
		fail("read first byte failed");
	n = read(sv[1], buf, sizeof(buf));
	if (n != 0)
		fail("read after drain should return 0 (EOF)");

	close(sv[0]);
	close(sv[1]);
	return 0;
}
