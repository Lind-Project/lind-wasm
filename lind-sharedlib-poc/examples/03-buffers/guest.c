// Example 03 — buffers. Caller-allocated OUTPUT buffers: the guest writes into
// memory the host provides, and the host copies the result back out (copy-out).
//
// The four functions each demonstrate a different "how many bytes were written"
// contract — the fact the C type can't express and that the manifest must declare:
//
//   to_upper      len = return value        (bytes written is the return)
//   greet         len = NUL-terminated      (out is a C string)
//   fill_pattern  len = whole capacity      (the function fills the buffer)
//   extract_word  len = a size_t* out-param (the function reports the length)
//
// Uses real libc (snprintf/toupper/malloc), so libc/libm are preloaded (see the
// Makefile PRELOAD). guest_malloc/guest_free let the host place data into and take
// data out of the guest's linear memory.

#include <stddef.h>
#include <stdlib.h>
#include <string.h>
//#include <ctype.h>
#include <stdio.h>

__attribute__((export_name("guest_malloc")))
void *guest_malloc(size_t n) { return malloc(n); }

__attribute__((export_name("guest_free")))
void guest_free(void *p) { free(p); }


// (1) len=ret — uppercase `in` into `out` (capacity n); return bytes written.
// Uppercase by hand rather than via libc `toupper`: glibc's ctype functions read
// the locale's ctype table, which isn't initialized in this reactor context (it
// would return 0). Doing it directly keeps the example about marshalling.
__attribute__((export_name("to_upper")))
size_t to_upper(const char *in, char *out, size_t n) {
    size_t i = 0;
    for (; in[i] && i < n; i++) {
        char c = in[i];
        if (c >= 'a' && c <= 'z') c -= 'a' - 'A';
        out[i] = c;
    }
    return i;
}

// (1) len=ret — uppercase `in` into `out` (capacity n); return bytes written.
//__attribute__((export_name("to_upper")))
//size_t to_upper(const char *in, char *out, size_t n) {
//    size_t i = 0;
//    for (; in[i] && i < n; i++) out[i] = (char)toupper((unsigned char)in[i]);
//    return i;
//}

// (2) len=nul — write "Hello, <name>!" into `out` as a NUL-terminated C string.
__attribute__((export_name("greet")))
void greet(const char *name, char *out, size_t n) {
    snprintf(out, n, "Hello, %s!", name);
}

// (3) len=cap — fill the whole buffer with a repeating A..Z pattern.
__attribute__((export_name("fill_pattern")))
void fill_pattern(char *out, size_t n) {
    for (size_t i = 0; i < n; i++) out[i] = (char)('A' + (i % 26));
}

// (4) len=arg — copy the first word of `in` into `out`; report its length in *out_len.
__attribute__((export_name("extract_word")))
void extract_word(const char *in, char *out, size_t n, size_t *out_len) {
    size_t i = 0;
    while (in[i] && in[i] != ' ' && i < n) {
        out[i] = in[i];
        i++;
    }
    *out_len = i;
}
