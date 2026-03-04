#include "bench.h"
#include <fcntl.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <stdio.h>

#define LOOP_COUNT(size) ((size) > 4096 ? 1000 : 1000000)

void write_size(size_t count) {
	char *buf = malloc(count); // [MAX];
	if (buf == NULL) {
		perror("malloc");
		exit(1);
	}

	memset(buf, 'A' + (count % 26), count);

	int fd = open("fs_write.txt", O_CREAT | O_WRONLY, 0644);

	int loops = LOOP_COUNT(count);

	long long start_time = gettimens();
	for (int i = 0; i < loops; i++) {
		pwrite(fd, buf, count, 0);
	}
	long long end_time = gettimens();

	long long total_time = end_time - start_time;

	close(fd);
	free(buf);

	long long avg_time = total_time / loops;

	emit_result("Write", count, avg_time, loops);
}

int main(int argc, char *argv[]) {
	int sizes[4] = {1, KiB(4), KiB(10), MiB(1)}; // , MiB(10)};

	for (int i = 0; i < 4; i++) {
		write_size(sizes[i]);
	}

	unlink("fs_write.txt");
}
