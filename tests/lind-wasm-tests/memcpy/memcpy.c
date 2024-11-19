#include <stdio.h>
#include <string.h>
#include <unistd.h>

int main() {
    // Source and destination buffers
    char src[] = "Hello, World!";
    char dst[50];

    // Use memcpy to copy data from src to dst
    memcpy(dst, src, strlen(src) + 1);  // +1 to include the null terminator

    // Use the write syscall to write the dst buffer to stdout
    write(1, dst, strlen(dst));

    // Return success
    return 0;
}

