/*
* Before running this test:
*   1. Create a file named "testfile.txt" in the $LIND_FS_ROOT directory.
*/

#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>

int main() {
    int fd = open("testfile.txt", O_RDONLY);  

    if (fd == -1) {
        perror("open failed");
        return 1;
    }

    printf("File opened successfully with fd = %d\n", fd);

    close(fd);
    return 0;
}
