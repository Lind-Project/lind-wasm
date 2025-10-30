#ifndef _LIND_ADDR_TRANSLATION_H
#define _LIND_ADDR_TRANSLATION_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// Imported host function to get the base address of the current cage's linear memory
// Module: "lind", name: "lind-get-memory-base"
unsigned long long __imported_lind_get_memory_base(void) __attribute__((
    __import_module__("lind"),
    __import_name__("lind-get-memory-base")
));

// Imported host function to get the current cage id (pid)
// Module: "lind", name: "lind-get-cage-id"
unsigned long long __imported_lind_get_cage_id(void) __attribute__((
    __import_module__("lind"),
    __import_name__("lind-get-cage-id")
));

// Cached base address for this process (cage). Set once per instance.
extern uint64_t __lind_base;

// Cached cage id (pid) for this process (cage). Set once per instance.
extern uint64_t __lind_cageid;

// Initialize address translation (idempotent). Queries base from host once.
void __lind_init_addr_translation(void);

// Check if a pointer is properly aligned for futex operations (must be 4-byte aligned)
// Returns: 1 if aligned, 0 if misaligned or NULL
int CHECK_FUTEX_ALIGNMENT(const void *host_ptr);

// Check if a pointer is non-NULL (for better error reporting)
// Returns: 1 if non-NULL, 0 if NULL
static inline int CHECK_PTR_NOT_NULL(const void *p) {
    return (p != NULL) ? 1 : 0;
}

// Translate a guest pointer (offset in wasm32 linear memory) to a host pointer (u64)
// Returns 0ULL if pointer is NULL, otherwise returns translated address
static inline uint64_t __lind_translate_ptr_to_host(const void *p) {
    if (p == NULL) return 0ULL;
    // Cast pointer value as an offset within the linear memory and add base
    return __lind_base + (uint64_t)(uintptr_t)p;
}

// Convenience macro for call sites
#define TRANSLATE_GUEST_POINTER_TO_HOST(p) __lind_translate_ptr_to_host((const void*)(p))

#ifdef __cplusplus
}
#endif

#endif // _LIND_ADDR_TRANSLATION_H
