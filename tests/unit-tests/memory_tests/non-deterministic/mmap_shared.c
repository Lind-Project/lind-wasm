#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>

int main() {
    int *addr1 = mmap(NULL, 10, PROT_READ | PROT_WRITE,
                      MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    int *addr2 = mmap(NULL, 10, PROT_READ | PROT_WRITE,
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    
    if(addr1 < 0)
    {
        perror("mmap");
        exit(1);
    }

    if(addr2 < 0)
    {
        perror("mmap");
        exit(1);
    }

    *addr1 = 1234;
    *addr2 = 4321;
    printf("parent value: %d, %d\n", *addr1, *addr2);

    if(fork()) {
        // parent
        printf("parent value after fork: %d, %d\n", *addr1, *addr2);
        sleep(1);
        *addr1 = 2333;
        *addr2 = 3332;
        printf("parent value after modification: %d, %d\n", *addr1, *addr2);
    } else {
        // child
        printf("child value after fork: %d, %d\n", *addr1, *addr2);
        sleep(2);
        printf("child value after modification: %d, %d\n", *addr1, *addr2);
    }

    return 0;
}
