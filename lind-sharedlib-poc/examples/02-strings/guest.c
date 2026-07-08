// Example 02 — strings. The first function that crosses the guest memory boundary.
//
// str_len takes a `const char*`. In the sandbox that pointer is an offset into the
// guest's OWN linear memory, so the host cannot just hand over a native address —
// it must copy the string into guest memory first (marshalling). To let the host
// place data into guest memory, the guest exposes its allocator as two exports.
//
// The function is deliberately NOT named `strlen`: the host .so must not export a
// symbol that shadows libc's own `strlen` for the whole process.

#include <stddef.h>
#include <stdlib.h>
#include <string.h>

// Host-callable allocator, so the host can reserve guest memory and copy bytes in.
// In wasm32 a pointer is an i32 offset, so these appear to the host as (i32)->i32
// and (i32)->void.
__attribute__((export_name("guest_malloc")))
void *guest_malloc(size_t n) { return malloc(n); }

__attribute__((export_name("guest_free")))
void guest_free(void *p) { free(p); }

// The demonstrated function: length of a marshalled C string.
__attribute__((export_name("str_len")))
size_t str_len(const char *s) { return strlen(s); }
