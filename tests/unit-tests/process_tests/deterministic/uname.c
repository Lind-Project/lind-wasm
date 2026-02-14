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
    
    assert(sysinfo.sysname != NULL); // Linux
    assert(sysinfo.release != NULL); // unknown
    assert(sysinfo.version != NULL); // unknown
    assert(sysinfo.machine != NULL); // x86_64
    assert(sysinfo.nodename != NULL);

    printf("uname test PASS\n");

    return 0;
}
