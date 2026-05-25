#include <stdio.h>

// Declarations for functions provided by the toy shared library.
// At runtime these are intercepted by the remote-lib wrapper and
// dispatched to the remote server based on routing_config.json.
int add(int a, int b);
int mul(int a, int b);

int main(void) {
    printf("add(3, 4)  = %d\n", add(3, 4));
    printf("mul(6, 7)  = %d\n", mul(6, 7));
    printf("add(10, -3) = %d\n", add(10, -3));
    return 0;
}
