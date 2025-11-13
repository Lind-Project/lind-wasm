#include <stdio.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    int ret = geteuid();
    printf("[Cage | geteuid] geteuid ret = %d\n", ret);
    return 0;
}
