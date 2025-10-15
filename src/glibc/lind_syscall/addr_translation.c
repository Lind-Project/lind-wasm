#include "addr_translation.h"

// Cached base, initialized on first call
uint64_t __lind_base = 0ULL;
// Cached cage id (pid), initialized on first call
uint64_t __lind_cageid = 0ULL;

void __lind_init_addr_translation(void) {
    if (__lind_base != 0ULL && __lind_cageid != 0ULL) {
        return; // idempotent
    }
    // Retrieve both the base address and the cage id
    __lind_meminfo_t info = __imported_lind_get_memory_base();
    __lind_base = (uint64_t)info.base;
    __lind_cageid = (uint64_t)info.cageid;
}
