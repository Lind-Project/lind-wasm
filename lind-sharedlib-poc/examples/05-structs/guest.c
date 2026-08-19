// Example 05 — structs (input). A `const struct Person *` argument crosses the
// sandbox boundary. Unlike a flat buffer, a struct can't be copied byte-for-byte:
// the host is LP64 (8-byte `long`/pointers) and the guest is wasm32/ILP32 (4-byte
// `long`/pointers), so the field *offsets* differ. The host stub reads each field
// with the host layout and hands them over; the engine repacks them into the guest
// layout, allocating the `char *name` string separately (nested copy-in).
//
//   struct Person { int id; long score; char *name; };
//     host  (LP64):  id@0  score@8  name@16   size 24
//     guest (ILP32): id@0  score@4  name@8    size 12
//
// Each function isolates one field so the demo shows every field type crossed
// correctly: an int, a width-converted long, and a nested string pointer.
//
// Uses real libc (strlen/malloc), so libc/libm are preloaded (see the Makefile
// PRELOAD). guest_malloc/guest_free let the host place data into the guest's memory.

#include <stddef.h>
#include <stdlib.h>
#include <string.h>

struct Person {
    int   id;
    long  score;
    char *name;
};

__attribute__((export_name("guest_malloc")))
void *guest_malloc(size_t n) { return malloc(n); }

__attribute__((export_name("guest_free")))
void guest_free(void *p) { free(p); }

// int field — same width on both sides.
__attribute__((export_name("person_id")))
int person_id(const struct Person *p) { return p->id; }

// long field — 8 bytes on the host, 4 in the guest (width conversion).
__attribute__((export_name("person_score")))
size_t person_score(const struct Person *p) { return (size_t)p->score; }

// pointer field — the string was nested-allocated into guest memory; prove it
// arrived intact by measuring it inside the sandbox.
__attribute__((export_name("person_namelen")))
size_t person_namelen(const struct Person *p) { return strlen(p->name); }
