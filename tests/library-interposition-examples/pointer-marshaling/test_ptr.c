#include <stdio.h>
#include <unistd.h>

// write(fd, buf, count) — intercepted by remote-lib.
// The buffer is sent to lind-remote-server, which calls native write().
// The message appears on the server's stdout; the return value comes back here.
int main(void) {
    const char msg[] = "hello from remote write!\n";
    int n = write(1, msg, sizeof(msg) - 1);
    // printf is not intercepted (glibc-internal write), so this appears in the cage.
    printf("write returned %d (expected %d)\n", n, (int)(sizeof(msg) - 1));
    return 0;
}
