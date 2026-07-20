/* Lind paravirtualized pshared semaphores.

   Process-shared semaphores cannot use the normal glibc implementation
   (value in shared memory + futex) on platforms where lind cannot alias the
   backing pages across cages (e.g. inside an SGX enclave, where the EPCM
   binds each page to a single linear address and the in-enclave futex is
   keyed by raw virtual address).

   Instead, the authoritative semaphore state lives in rawposix, keyed by the
   (shared region, offset) the sem_t lives at; rawposix derives that key from
   the caller's vmmap.  These wrappers forward the sem_t address (translated
   to a host address, like every other pointer argument) plus the operation
   parameters.  They are used by nptl/sem_*.c only when the semaphore was
   created with pshared != 0; process-private semaphores keep the userspace
   fast path.

   Errno translation is ON: the wrappers return 0 / -1-with-errno, matching
   the public sem_* API contract.  */

#ifndef _LIND_SEM_H
#define _LIND_SEM_H 1

#include <stdint.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>
#include <lind_constants.h>

/* Wait flavors; must match SEM_WAIT_* in rawposix's sem_calls.rs.  */
#define LIND_SEM_WAIT_BLOCK 0
#define LIND_SEM_WAIT_TRY   1
#define LIND_SEM_WAIT_TIMED 2

static inline int
__lind_sem_init (void *sem, unsigned int value)
{
  return MAKE_LEGACY_SYSCALL (SEM_INIT_SYSCALL, "syscall|sem_init",
			      TRANSLATE_GUEST_POINTER_TO_HOST (sem),
			      (uint64_t) value, 0, 0, 0, 0,
			      TRANSLATE_ERRNO_ON);
}

/* flags: LIND_SEM_WAIT_*.  abs_sec/abs_nsec: absolute timeout on clockid
   (only read for LIND_SEM_WAIT_TIMED).  */
static inline int
__lind_sem_wait (void *sem, int flags, int64_t abs_sec, int64_t abs_nsec,
		 int clockid)
{
  return MAKE_LEGACY_SYSCALL (SEM_WAIT_SYSCALL, "syscall|sem_wait",
			      TRANSLATE_GUEST_POINTER_TO_HOST (sem),
			      (uint64_t) flags, (uint64_t) abs_sec,
			      (uint64_t) abs_nsec, (uint64_t) clockid, 0,
			      TRANSLATE_ERRNO_ON);
}

static inline int
__lind_sem_post (void *sem)
{
  return MAKE_LEGACY_SYSCALL (SEM_POST_SYSCALL, "syscall|sem_post",
			      TRANSLATE_GUEST_POINTER_TO_HOST (sem),
			      0, 0, 0, 0, 0,
			      TRANSLATE_ERRNO_ON);
}

/* Returns the current value (>= 0), or -1 with errno set.  */
static inline int
__lind_sem_getvalue (void *sem)
{
  return MAKE_LEGACY_SYSCALL (SEM_GETVALUE_SYSCALL, "syscall|sem_getvalue",
			      TRANSLATE_GUEST_POINTER_TO_HOST (sem),
			      0, 0, 0, 0, 0,
			      TRANSLATE_ERRNO_ON);
}

static inline int
__lind_sem_destroy (void *sem)
{
  return MAKE_LEGACY_SYSCALL (SEM_DESTROY_SYSCALL, "syscall|sem_destroy",
			      TRANSLATE_GUEST_POINTER_TO_HOST (sem),
			      0, 0, 0, 0, 0,
			      TRANSLATE_ERRNO_ON);
}

#endif /* _LIND_SEM_H */
