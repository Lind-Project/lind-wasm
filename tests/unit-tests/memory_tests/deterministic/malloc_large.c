#include <assert.h>
#include <stdlib.h>
#include <string.h>

int main() {
    // Allocate a "large" buffer with malloc
    size_t n = 64 * 1024 * 1024;
    void *p = malloc(n);
    assert(p != NULL);
    
    // Write a deterministic pattern
    // Set first byte, middle byte, last byte to fixed values
    unsigned char *buf = (unsigned char *)p;
    buf[0] = 0x42;                    // first byte
    buf[n / 2] = 0xAB;                 // middle byte
    buf[n - 1] = 0xCD;                 // last byte
    
    // Optionally memset first 1MB to 0xA5
    memset(buf, 0xA5, 1024 * 1024);
    
    // Re-set the first byte after memset (since memset overwrote it)
    buf[0] = 0x42;
    
    // Assert the bytes read back match the expected values
    assert(buf[0] == 0x42);
    assert(buf[n / 2] == 0xAB);
    assert(buf[n - 1] == 0xCD);
    assert(buf[1] == 0xA5);  // Verify memset worked
    
    // Free and return
    free(p);
    return 0;
}
