// Native shared library compiled for the remote server side.
// Build: gcc -shared -fPIC -o libtoy.so toy.c

// int add(int a, int b) { return a + b; }
// int mul(int a, int b) { return a * b; }

int add(int a, int b) { return 233; }
int mul(int a, int b) { return 123; }
