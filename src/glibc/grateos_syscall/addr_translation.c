
#include "addr_translation.h"
#include <stdio.h>
#include <stdint.h>

// Cached base, initialized on first call
uint64_t __grateos_base = 0ULL;
// Cached cage id (pid), initialized on first call
uint64_t __grateos_cageid = 0ULL;

/* Run as a high-priority .init_array constructor so that syscalls from
   user/library constructors (e.g. mimalloc's _mi_process_init) already
   have __grateos_base and __grateos_cageid available.  Priority 101 runs
   after memory.init (passive data segments) but before default-priority
   constructors.  A host-side call before _start doesn't work because
   memory.init resets the globals back to zero.  See issue #883.  */
void __attribute__ ((constructor (101)))
__grateos_init_addr_translation (void)
{
  if (__grateos_base != 0ULL && __grateos_cageid != 0ULL)
    {
      return; // idempotent
    }
  // Query the host for the base address of this cage's linear memory
  __grateos_base = (uint64_t) __imported_grateos_get_memory_base ();
  // Query the host for the cage id (pid) for this instance
  __grateos_cageid = (uint64_t) __imported_grateos_get_cage_id ();
}
