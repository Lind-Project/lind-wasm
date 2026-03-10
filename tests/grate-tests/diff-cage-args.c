#include <stdio.h>
#include <unistd.h>
#include <fcntl.h>

int main() {
	int fd = open("redirected.txt", O_RDONLY, 0);
	printf("Hello world. FD=%d\n", fd);

	char buf[10];
	int ret = read(1, buf, 10);

	printf("Goodbye world! ret=%d buf=%s\n", ret, buf);
}
