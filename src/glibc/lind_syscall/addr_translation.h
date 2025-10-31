#ifndef _LIND_ADDR_TRANSLATION_H
#define _LIND_ADDR_TRANSLATION_H

#include <stdint.h>
#include <stddef.h>
#include <errno.h>

#ifdef __cplusplus
extern "C"
{
#endif

  // Imported host function to get the base address of the current cage's
  // linear memory Module: "lind", name: "lind-get-memory-base"
  unsigned long long __imported_lind_get_memory_base (void)
      __attribute__ ((__import_module__ ("lind"),
		      __import_name__ ("lind-get-memory-base")));

  // Imported host function to get the current cage id (pid)
  // Module: "lind", name: "lind-get-cage-id"
  unsigned long long __imported_lind_get_cage_id (void)
      __attribute__ ((__import_module__ ("lind"),
		      __import_name__ ("lind-get-cage-id")));

  // Cached base address for this process (cage). Set once per instance.
  extern uint64_t __lind_base;

  // Cached cage id (pid) for this process (cage). Set once per instance.
  extern uint64_t __lind_cageid;

  // Initialize address translation (idempotent). Queries base from host once.
  void __lind_init_addr_translation (void);

  // Translate a guest pointer (offset in wasm32 linear memory) to a host
  // pointer (u64)
  static inline uint64_t
  __lind_translate_ptr_to_host (const void *p)
  {
    if (p == NULL)
      return 0ULL;
    // Cast pointer value as an offset within the linear memory and add base
    return __lind_base + (uint64_t) (uintptr_t) p;
  }

// Convenience macro for call sites
#define TRANSLATE_GUEST_POINTER_TO_HOST(p)                                    \
  __lind_translate_ptr_to_host ((const void *) (p))


// Helper macro to check if a translated host pointer is null and return error
// Used for mandatory non-nullable pointer parameters
// Returns -1 and sets errno to EFAULT if the pointer is null
// name param is unused, mainly for better readibility in call sites
#define CHECK_NULL_PTR(ptr, name)                                             \
  do                                                                          \
    {                                                                         \
      if ((ptr) == 0ULL)                                                      \
	{                                                                     \
	  errno = EFAULT;                                                     \
	  return -1;                                                          \
	}                                                                     \
    }                                                                         \
  while (0)

// Helper macro for checking buffer pointers (used in read/write/etc.)
// Only checks if count > 0, allows NULL buffers otherwise
#define CHECK_NULL_BUF(ptr, count)                                            \
  do                                                                          \
    {                                                                         \
      if ((count) > 0 && (ptr) == 0ULL)                                       \
	{                                                                     \
	  errno = EFAULT;                                                     \
	  return -1;                                                          \
	}                                                                     \
    }                                                                         \
  while (0)

#ifdef __cplusplus
}
#endif

#endif // _LIND_ADDR_TRANSLATION_H
