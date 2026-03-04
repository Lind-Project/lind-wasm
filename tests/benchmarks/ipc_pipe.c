#include "bench.h"
#include <stdio.h>
#include <unistd.h>
#include <stdlib.h>
#include <fcntl.h>
#include <time.h>
#include <string.h>
#include <sys/wait.h>

#define LOOP_COUNT(size) ((size) > 4096 ? 1000 : 1000000)

void bench_pipe(int msg_size) {
	int p2c[2], c2p[2];

	int loops = LOOP_COUNT(msg_size);

	if (pipe(p2c) || pipe(c2p)) {
		perror("pipe");
		exit(1);
	}

	pid_t pid = fork();
	if (pid < 0) {
		perror("fork");
		exit(1);
	}

	// Child
	if (pid == 0) {
		close(p2c[1]);
		close(c2p[0]);

		// char buf[msg_size];
		char *buf = malloc(msg_size);
		if (buf == NULL) {
			exit(0);
		}
		for (int i = 0; i < loops; i++) {
			ssize_t n = read(p2c[0], buf, msg_size);
			if (n <= 0) {
				fprintf(stderr, "0 bytes read\n");
				exit(1);
			}
			write(c2p[1], buf, n);
		}

		free(buf);

		close(p2c[0]);
		close(c2p[0]);
		_exit(0);
	}

	// Parent
	close(p2c[0]);
	close(c2p[1]);
	// char buf[msg_size];
	char *buf = malloc(msg_size);
	if (buf == NULL) {
		exit(0);
	}
	memset(buf, 0x42, msg_size);

	long long t0 = gettimens();
	for (int i = 0; i < loops; i++) {
		write(p2c[1], buf, msg_size);
		read(c2p[0], buf, msg_size);
	}
	long long t1 = gettimens();

	free(buf);

	close(p2c[1]);
	close(c2p[0]);
	wait(NULL);

	emit_result("Pipe (RTT)", msg_size, (t1 - t0) / loops, loops);
}

int main() {
	int sizes[] = {1, KiB(4), KiB(16), KiB(32)};

	for (int i = 0; i < 4; i++) {
		bench_pipe(sizes[i]);
	}
}
