#include <stdio.h>
#include <stdlib.h>

// rand() is imported from glibc (loaded automatically by the runtime).
// When routing_config.json routes "rand" to the remote server, each call
// goes over the Unix socket RPC and the server's custom rand() returns the
// sentinel value 42424242 instead of a pseudo-random number.
//
// Expected output with remote routing active:
//   rand() = 42424242
//   rand() = 42424242
//   rand() = 42424242
//
// Expected output without remote routing (local glibc):
//   rand() = <pseudo-random numbers>

int main(void) {
    printf("rand() = %d\n", rand());
    printf("rand() = %d\n", rand());
    printf("rand() = %d\n", rand());
    return 0;
}
