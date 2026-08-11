// Example 04 — in/out buffers. The guest both READS and WRITES a buffer the caller
// allocates: in-place transforms. This is the union of 02 (copy-in) and 03
// (copy-out) on a single argument — the host seeds the guest allocation with the
// caller's current bytes, runs the call, then copies the result back.
//
// The same four "how many bytes came back" contracts as 03 apply, so each function
// picks a different one:
//
//   compact_spaces  len = return value        (bytes written is the return)
//   reverse_str     len = NUL-terminated      (buffer holds a C string)
//   rot13_block     len = whole capacity      (fixed-size block transform)
//   append_suffix   len = a size_t* out-param (also takes a copy-in `const char*`)
//
// Uses real libc (strnlen/malloc), so libc/libm are preloaded (see the Makefile
// PRELOAD). guest_malloc/guest_free let the host place data into and take data out
// of the guest's linear memory.

#include <stddef.h>
#include <stdlib.h>
#include <string.h>

__attribute__((export_name("guest_malloc")))
void *guest_malloc(size_t n) { return malloc(n); }

__attribute__((export_name("guest_free")))
void guest_free(void *p) { free(p); }

// (1) len=ret — collapse runs of spaces in place; return the new length.
__attribute__((export_name("compact_spaces")))
size_t compact_spaces(char *buf, size_t n) {
    size_t len = strnlen(buf, n);
    size_t w = 0;
    int prev_space = 0;
    for (size_t r = 0; r < len; r++) {
        int is_space = (buf[r] == ' ');
        if (is_space && prev_space) continue;
        buf[w++] = buf[r];
        prev_space = is_space;
    }
    if (w < n) buf[w] = '\0';
    return w;
}

// (2) len=nul — reverse the C string held in the buffer, in place.
__attribute__((export_name("reverse_str")))
void reverse_str(char *buf, size_t n) {
    size_t len = strnlen(buf, n);
    if (len < 2) return;
    for (size_t i = 0, j = len - 1; i < j; i++, j--) {
        char t = buf[i];
        buf[i] = buf[j];
        buf[j] = t;
    }
}

// (3) len=cap — ROT13 every byte of the buffer. Not a string operation: the whole
// capacity is transformed, and applying it twice restores the original.
__attribute__((export_name("rot13_block")))
void rot13_block(char *buf, size_t n) {
    for (size_t i = 0; i < n; i++) {
        char c = buf[i];
        if (c >= 'a' && c <= 'z') buf[i] = (char)('a' + (c - 'a' + 13) % 26);
        else if (c >= 'A' && c <= 'Z') buf[i] = (char)('A' + (c - 'A' + 13) % 26);
    }
}

// (4) len=arg — append `suffix` to the string already in the buffer (an in/out
// buffer alongside a copy-in input), reporting the new length in *out_len.
__attribute__((export_name("append_suffix")))
void append_suffix(char *buf, size_t n, const char *suffix, size_t *out_len) {
    size_t len = strnlen(buf, n);
    size_t i = 0;
    while (len + 1 < n && suffix[i]) buf[len++] = suffix[i++];
    if (len < n) buf[len] = '\0';
    *out_len = len;
}
