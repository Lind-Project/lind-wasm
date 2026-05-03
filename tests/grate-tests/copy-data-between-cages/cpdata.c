#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>

int main() {
	int fd = open("random", O_CREAT | O_RDONLY, 0544);

    // We don't test redirecting the open call. We just want 
    // to make sure that the open call goes through and the data 
    // is copied correctly, so the return value here should be 
    // arbitrary number defined in grate, not the actual fd for 
    // "random".
	printf("[cage] fd=%d\n", fd);
}
