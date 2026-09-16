// Example 07 — callbacks (guest -> host upcall).
//
// The reverse direction of every earlier example: instead of the host calling a
// guest export, the GUEST calls a host function. It imports one generic entry
// point, `__lind_upcall(slot, a0, a1, a2)`, which the lind runtime routes to a
// handler the stub registered (see stub/src/lib.rs). This is the primitive the
// OpenBLAS xerbla error handler needs: OpenBLAS-in-wasm calling back out to the
// host's error handler.
//
// Undefined here, so `lind_compile` turns it into a dynamic import `env.__lind_upcall`
// (the same way example 06's shim imports cblas_* from the preloaded OpenBLAS).
extern int __lind_upcall(int slot, int a0, int a1, int a2);

// slot 0 — pass a scalar OUT to the host and use its result. The registered host
// handler doubles it; we add 1 so the round trip is visible on both ends.
__attribute__((export_name("apply_cb")))
int apply_cb(int x) {
    int from_host = __lind_upcall(0, x, 0, 0);
    return from_host + 1;
}

// slot 1 — hand the host a POINTER into guest memory plus a length. The host reads
// the string back out of the sandbox (reverse marshalling) and returns its length,
// proving a host handler can pull a guest buffer across the boundary.
__attribute__((export_name("report_name")))
int report_name(void) {
    static const char name[] = "hello-from-guest";
    return __lind_upcall(1, (int)(long)name, (int)(sizeof(name) - 1), 0);
}

#ifndef __wasm__
// Native baseline ONLY (the `make run-native` control build): a stand-in that
// mirrors the host handlers so the unmodified demo links and behaves identically.
// Not compiled for the wasm guest, which imports __lind_upcall from the host.
int __lind_upcall(int slot, int a0, int a1, int a2) {
    (void)a2;
    if (slot == 0) return a0 * 2;  // matches the stub's slot-0 handler
    if (slot == 1) return a1;      // matches the stub's slot-1 handler (return length)
    return 0;
}
#endif
