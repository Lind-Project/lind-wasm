// Server-side native library for the remote-calls-rand example.
// Build: gcc -shared -fPIC -o librand.so rand_lib.c
//
// Provides a sentinel rand() that always returns a fixed value (42424242),
// making it immediately obvious when a call was served by the remote server
// rather than the local glibc.

int rand(void) { return 42424242; }
