#include <stdio.h>
#include <sys/utsname.h>
#include <assert.h>
#include <string.h>

int main() {
    struct utsname sysinfo;

    if (uname(&sysinfo) == -1) {
        perror("uname");
        return 1;
    }

    assert(strcmp(sysinfo.sysname, "Linux") == 0);
    assert(strcmp(sysinfo.release, "unknown") == 0);
    assert(strcmp(sysinfo.version, "unknown") == 0);
    assert(strcmp(sysinfo.machine, "x86_64") == 0);
    assert(sysinfo.nodename != NULL);

    printf("uname test PASS\n");

    return 0;
}
