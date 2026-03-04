#include "bench.h"
#include <stdio.h>
#include <unistd.h>
#include <stdlib.h>
#include <fcntl.h>
#include <time.h>
#include <string.h>
#include <sys/wait.h>
#include <sys/socket.h>

#define LOOP_COUNT(size) ((size) > 4096 ? 1000 : 100000)

void uds_dgram(int msg_size) {
	int sv[2];
	if (socketpair(AF_UNIX, SOCK_DGRAM, 0, sv)) {
		perror("socketpair");
		exit(1);
	}

	int loops = LOOP_COUNT(msg_size);

	pid_t pid = fork();

	if (pid < 0) {
		perror("fork");
		exit(1);
	}

	// Child
	if (pid == 0) {
		close(sv[0]);
		char *buf = malloc(msg_size);
		if (buf == NULL) {
			exit(1);
		}
		for (int i = 0; i < loops; i++) {
			ssize_t n = recv(sv[1], buf, msg_size, 0);
			if (n <= 0) {
				fprintf(stderr, "Received 0 bytes\n");
				exit(1);
			}
			send(sv[1], buf, n, 0);
		}
		close(sv[1]);
		exit(0);
	}

	// Parent
	close(sv[1]);
	char *buf = malloc(msg_size);
	if (buf == NULL) {
		exit(1);
	}
	memset(buf, 0x42, msg_size);

	long long start = gettimens();
	for (int i = 0; i < loops; i++) {
		send(sv[0], buf, msg_size, 0);
		recv(sv[0], buf, msg_size, 0);
	}
	long long end = gettimens();

	free(buf);

	emit_result("Unix Domain Socket (DGRAM) - RTT", msg_size,
		    (end - start) / loops, loops);
}

void uds_stream(int msg_size) {
	int sv[2];
	if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv)) {
		perror("socketpair");
		exit(1);
	}

	int loops = LOOP_COUNT(msg_size);
	pid_t pid = fork();

	if (pid < 0) {
		perror("fork");
		exit(1);
	}

	// Child
	if (pid == 0) {
		close(sv[0]);
		char *buf = malloc(msg_size);
		if (buf == NULL) {
			exit(1);
		}
		for (int i = 0; i < loops; i++) {
			ssize_t n = recv(sv[1], buf, msg_size, 0);
			if (n <= 0) {
				fprintf(stderr, "Received 0 bytes\n");
				exit(1);
			}
			send(sv[1], buf, n, 0);
		}
		close(sv[1]);
		free(buf);
		exit(0);
	}

	// Parent
	close(sv[1]);
	char *buf = malloc(msg_size);
	if (buf == NULL) {
		exit(0);
	}
	memset(buf, 0x42, msg_size);

	long long start = gettimens();
	for (int i = 0; i < loops; i++) {
		send(sv[0], buf, msg_size, 0);
		recv(sv[0], buf, msg_size, 0);
	}
	long long end = gettimens();

	free(buf);

	emit_result("Unix Domain Socket (STREAM) - RTT", msg_size,
		    (end - start) / loops, loops);
}

int main() {
	int sizes[] = {1, KiB(4), KiB(16), KiB(32)};

	for (int i = 0; i < 4; i++) {
		uds_stream(sizes[i]);
		uds_dgram(sizes[i]);
	}
}
