#include <stdio.h>
#include <unistd.h>
#include <fcntl.h>

int main() {
	int fd = open("redirected.txt", O_RDONLY, 0);
	printf("Hello world. FD=%d\n", fd);

	char buf[11];
	int ret = read(fd, buf, 10);
	buf[ret] = '\0';

	printf("Goodbye world! ret=%d buf=%s\n", ret, buf);
}
