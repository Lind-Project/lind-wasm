
#include "addr_translation.h"
#include <stdio.h>
#include <stdint.h>

// Cached base, initialized on first call
uint64_t __lind_base = 0ULL;
// Cached cage id (pid), initialized on first call
uint64_t __lind_cageid = 0ULL;

void
__lind_init_addr_translation (void)
{
  if (__lind_base != 0ULL && __lind_cageid != 0ULL)
    {
      return; // idempotent
    }
  // Query the host for the base address of this cage's linear memory
  __lind_base = (uint64_t) __imported_lind_get_memory_base ();
  // Query the host for the cage id (pid) for this instance
  __lind_cageid = (uint64_t) __imported_lind_get_cage_id ();
}

int
CHECK_FUTEX_ALIGNMENT (const void *host_ptr)
{
  uintptr_t addr = (uintptr_t) host_ptr;
  if (addr % 4 != 0)
    {
      fprintf (stderr,
	       "[glibc-futex] WARNING: Misaligned futex host address: %p\n",
	       host_ptr);
      return 0;
    }
  return 1;
}