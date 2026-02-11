#include <stdio.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    int ret_euid = geteuid();
    int ret_uid = getuid();
    printf("[Cage | multi-register] geteuid ret = %d\n", ret_euid);
    printf("[Cage | multi-register] getuid ret = %d\n", ret_uid);
    return 0;
}
