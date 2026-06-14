// DESCRIPTION: Issues pread() for buffer of size PARAM.
#include "bench.h"
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <stdio.h>

void read_size(size_t count) {
	char *buf = malloc(count);

	int fd = open("tmp_fs_read.txt", O_RDONLY, 0);

	int loops = IO_LOOP_COUNT(count);

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
	// Create a temporary file of appropriate size to be read later.
	int max_size = 0;
	for (int i = 0; i < FS_SIZE_COUNT; i++) {
		if (max_size < fs_sizes[i])
			max_size = fs_sizes[i];
	}
	char wbuf[max_size];
	memset(wbuf, 'A', max_size);

	int fd = open("tmp_fs_read.txt", O_CREAT | O_WRONLY, 0666);
	write(fd, &wbuf, max_size);
	close(fd);

	// Run benchmarks.
	for (int i = 0; i < FS_SIZE_COUNT; i++) {
		read_size(fs_sizes[i]);
	}

	unlink("tmp_fs_read.txt");
}
