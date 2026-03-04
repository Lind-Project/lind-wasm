#include "bench.h"
#include <fcntl.h>
#include <stdlib.h>
#include <unistd.h>
#include <stdio.h>

#define LOOP_COUNT(size) ((size) > 4096 ? 1000 : 1000000)

void read_size(size_t count) {
	char *buf = malloc(count); // [MAX];

	int fd = open("tmp_fs_read.txt", O_RDONLY, 0);

	int loops = LOOP_COUNT(count);

	long long start_time = gettimens();
	for (int i = 0; i < loops; i++) {
		pread(fd, buf, count, 0);
	}
	long long end_time = gettimens();

	long long avg_time = (end_time - start_time) / loops;
	emit_result("Read", count, avg_time, loops);

	close(fd);
	free(buf);
}

int main(int argc, char *argv[]) {
	int sizes[4] = {1, KiB(1), KiB(4), KiB(10)}; // MiB(1), MiB(10)};

	char wchar = 'A';

	// Create a temporary file of appropriate size to be read later.
	int fd = open("tmp_fs_read.txt", O_CREAT | O_WRONLY, 0666);
	for (int i = 0; i < KiB(10); i++) {
		write(fd, &wchar, 1);
	}
	close(fd);

	// Run benchmarks.

	for (int i = 0; i < 4; i++) {
		read_size(sizes[i]);
	}

	read_size(4096);

	unlink("tmp_fs_read.txt");
}
