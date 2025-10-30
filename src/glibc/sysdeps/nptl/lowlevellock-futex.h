/* Low-level locking access to futex facilities.  Stub version.
   Copyright (C) 2014-2024 Free Software Foundation, Inc.
   This file is part of the GNU C Library.

   The GNU C Library is free software; you can redistribute it and/or
   modify it under the terms of the GNU Lesser General Public
   License as published by the Free Software Foundation; either
   version 2.1 of the License, or (at your option) any later version.

   The GNU C Library is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
   Lesser General Public License for more details.

   You should have received a copy of the GNU Lesser General Public
   License along with the GNU C Library.  If not, see
   <https://www.gnu.org/licenses/>.  */

#ifndef _LOWLEVELLOCK_FUTEX_H
#define _LOWLEVELLOCK_FUTEX_H   1

#include <syscall-template.h>
#include <lind_syscall_num.h>

#ifndef __ASSEMBLER__
# include <sysdep.h>
# include <sysdep-cancel.h>
# include <kernel-features.h>
# include <lind_syscall/addr_translation.h>
#endif

#define FUTEX_WAIT              0
#define FUTEX_WAKE              1
#define FUTEX_REQUEUE           3
#define FUTEX_CMP_REQUEUE       4
#define FUTEX_WAKE_OP           5
#define FUTEX_OP_CLEAR_WAKE_IF_GT_ONE ((4 << 24) | 1)
#define FUTEX_LOCK_PI           6
#define FUTEX_UNLOCK_PI         7
#define FUTEX_TRYLOCK_PI        8
#define FUTEX_WAIT_BITSET       9
#define FUTEX_WAKE_BITSET       10
#define FUTEX_WAIT_REQUEUE_PI   11
#define FUTEX_CMP_REQUEUE_PI    12
#define FUTEX_LOCK_PI2          13
#define FUTEX_PRIVATE_FLAG      128
#define FUTEX_CLOCK_REALTIME    256

#define FUTEX_BITSET_MATCH_ANY  0xffffffff

#define LLL_PRIVATE 0
#define LLL_SHARED  FUTEX_PRIVATE_FLAG

#ifndef __ASSEMBLER__
# define __lll_private_flag(fl, private) \
  (((fl) | FUTEX_PRIVATE_FLAG) ^ (private))

/* Primary futex syscall wrapper used in glibc-based locking macros.
   Returns a negated errno on failure, or 0 on success.  */
# define lll_futex_syscall(nargs, futexp, op, ...)                      \
  ({                                                                    \
    long int __ret = MAKE_RAW_SYSCALL##nargs (FUTEX_SYSCALL, "syscall|futex", \
                                              futexp, op, __VA_ARGS__); \
    (__glibc_unlikely (INTERNAL_SYSCALL_ERROR_P (__ret))                \
     ? -INTERNAL_SYSCALL_ERRNO (__ret) : 0);                            \
  })

/* Safe version of futex syscall that first translates the futex pointer
   from user space to host space (used in Lind WASM build).  */
# define __lll_futex_syscall_with_translated_ptrs(nargs, futexp, op, ...) \
  ({                                                                      \
    __lind_init_addr_translation();                                       \
    uint64_t __host_futex_ptr = TRANSLATE_GUEST_POINTER_TO_HOST(futexp);  \
    long int __ret;                                                       \
                                                                          \
    if (!__host_futex_ptr || !CHECK_FUTEX_ALIGNMENT(futexp)) {            \
      __ret = -EINVAL;                                                    \
    } else {                                                              \
      __ret = MAKE_RAW_SYSCALL##nargs(FUTEX_SYSCALL, "syscall|futex",    \
                                      __host_futex_ptr, op, __VA_ARGS__); \
    }                                                                     \
                                                                          \
    (__glibc_unlikely(INTERNAL_SYSCALL_ERROR_P(__ret))                    \
     ? -INTERNAL_SYSCALL_ERRNO(__ret)                                     \
     : 0);                                                                \
  })

/* Wait while *FUTEXP == VAL for an lll_futex_wake call on FUTEXP.  */
# define lll_futex_wait(futexp, val, private) \
  lll_futex_timed_wait(futexp, val, NULL, private)

# define lll_futex_timed_wait(futexp, val, timeout, private)     \
  ({                                                             \
    __lind_init_addr_translation();                              \
    /* Translate timeout pointer - NULL is valid (no timeout) */ \
    uint64_t __host_timeout = TRANSLATE_GUEST_POINTER_TO_HOST(timeout); \
    __lll_futex_syscall_with_translated_ptrs(4, futexp,         \
                    __lll_private_flag(FUTEX_WAIT, private),    \
                    val, __host_timeout);                       \
  })

/* Verify whether the supplied clockid is supported by
   lll_futex_clock_wait_bitset.  */
# define lll_futex_supported_clockid(clockid)			\
  ((clockid) == CLOCK_REALTIME || (clockid) == CLOCK_MONOTONIC)

/* Wake up up to NR waiters on FUTEXP.  */
# define lll_futex_wake(futexp, nr, private)                     \
  __lll_futex_syscall_with_translated_ptrs(4, futexp,           \
                    __lll_private_flag(FUTEX_WAKE, private), nr, 0)

/* Requeue waiters from FUTEXP to MUTEX.  */
# define lll_futex_requeue(futexp, nr_wake, nr_move, mutex, val, private) \
  ({                                                                      \
    __lind_init_addr_translation();                                       \
    long int __ret;                                                       \
                                                                          \
    /* Both futex pointers must be valid and aligned */                  \
    if (!CHECK_PTR_NOT_NULL(futexp) || !CHECK_FUTEX_ALIGNMENT(futexp) || \
        !CHECK_PTR_NOT_NULL(mutex) || !CHECK_FUTEX_ALIGNMENT(mutex)) {   \
      __ret = -EINVAL;                                                    \
    } else {                                                              \
      uint64_t __host_futex = TRANSLATE_GUEST_POINTER_TO_HOST(futexp);    \
      uint64_t __host_mutex = TRANSLATE_GUEST_POINTER_TO_HOST(mutex);     \
      __ret = MAKE_RAW_SYSCALL6(FUTEX_SYSCALL, "syscall|futex",          \
                                __host_futex,                             \
                                __lll_private_flag(FUTEX_CMP_REQUEUE, private), \
                                nr_wake, nr_move, __host_mutex, val);     \
    }                                                                     \
    (__glibc_unlikely(INTERNAL_SYSCALL_ERROR_P(__ret))                    \
     ? -INTERNAL_SYSCALL_ERRNO(__ret) : 0);                               \
  })

/* Wake up up to NR_WAKE waiters on FUTEXP and NR_WAKE2 on FUTEXP2.  */
# define lll_futex_wake_unlock(futexp, nr_wake, nr_wake2, futexp2, private) \
  ({                                                                      \
    __lind_init_addr_translation();                                       \
    long int __ret;                                                       \
                                                                          \
    /* Both futex pointers must be valid and aligned */                  \
    if (!CHECK_PTR_NOT_NULL(futexp) || !CHECK_FUTEX_ALIGNMENT(futexp) || \
        !CHECK_PTR_NOT_NULL(futexp2) || !CHECK_FUTEX_ALIGNMENT(futexp2)) { \
      __ret = -EINVAL;                                                    \
    } else {                                                              \
      uint64_t __host_futex = TRANSLATE_GUEST_POINTER_TO_HOST(futexp);    \
      uint64_t __host_futex2 = TRANSLATE_GUEST_POINTER_TO_HOST(futexp2);  \
      __ret = MAKE_RAW_SYSCALL6(FUTEX_SYSCALL, "syscall|futex",          \
                                __host_futex,                             \
                                __lll_private_flag(FUTEX_WAKE_OP, private), \
                                nr_wake, nr_wake2, __host_futex2,         \
                                FUTEX_OP_CLEAR_WAKE_IF_GT_ONE);           \
    }                                                                     \
    (__glibc_unlikely(INTERNAL_SYSCALL_ERROR_P(__ret))                    \
     ? -INTERNAL_SYSCALL_ERRNO(__ret) : 0);                               \
  })

/* Priority inheritance unlock.  */
# define lll_futex_timed_unlock_pi(futexp, private) 			\
  __lll_futex_syscall_with_translated_ptrs(4, futexp,			\
                    __lll_private_flag(FUTEX_UNLOCK_PI, private),	\
                    0, 0)

/* Like lll_futex_requeue, but pairs with lll_futex_wait_requeue_pi.  */
# define lll_futex_cmp_requeue_pi(futexp, nr_wake, nr_move, mutex, val, private) \
  ({                                                                      \
    __lind_init_addr_translation();                                       \
    long int __ret;                                                       \
                                                                          \
    /* Both futex pointers must be valid and aligned */                  \
    if (!CHECK_PTR_NOT_NULL(futexp) || !CHECK_FUTEX_ALIGNMENT(futexp) || \
        !CHECK_PTR_NOT_NULL(mutex) || !CHECK_FUTEX_ALIGNMENT(mutex)) {   \
      __ret = -EINVAL;                                                    \
    } else {                                                              \
      uint64_t __host_futex = TRANSLATE_GUEST_POINTER_TO_HOST(futexp);    \
      uint64_t __host_mutex = TRANSLATE_GUEST_POINTER_TO_HOST(mutex);     \
      __ret = MAKE_RAW_SYSCALL6(FUTEX_SYSCALL, "syscall|futex",          \
                                __host_futex,                             \
                                __lll_private_flag(FUTEX_CMP_REQUEUE_PI, private), \
                                nr_wake, nr_move, __host_mutex, val);     \
    }                                                                     \
    (__glibc_unlikely(INTERNAL_SYSCALL_ERROR_P(__ret))                    \
     ? -INTERNAL_SYSCALL_ERRNO(__ret) : 0);                               \
  })

/* Cancellable wait variants.  */
# define lll_futex_wait_cancel(futexp, val, private) \
  ({ int __oldtype = LIBC_CANCEL_ASYNC();            \
     long int __err = lll_futex_wait(futexp, val, LLL_SHARED); \
     LIBC_CANCEL_RESET(__oldtype);                   \
     __err; })

# define lll_futex_timed_wait_cancel(futexp, val, timeout, private) \
  ({ int __oldtype = LIBC_CANCEL_ASYNC();                           \
     long int __err = lll_futex_timed_wait(futexp, val, timeout, private); \
     LIBC_CANCEL_RESET(__oldtype);                                  \
     __err; })

#endif /* !__ASSEMBLER__ */

#endif /* _LOWLEVELLOCK_FUTEX_H */
