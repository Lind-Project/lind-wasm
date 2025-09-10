#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    int ret = open("/etc/hostname", O_RDONLY);
    printf("[Cage | geteuid] geteuid ret = %d\n", ret);
    return 0;
}
