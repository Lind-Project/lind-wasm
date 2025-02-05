/* Copyright (C) 2003-2024 Free Software Foundation, Inc.
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

#ifndef _ATOMIC_MACHINE_H
#define _ATOMIC_MACHINE_H	1

#define atomic_full_barrier() __sync_synchronize ()

#define __HAVE_64B_ATOMICS 0
#define USE_ATOMIC_COMPILER_BUILTINS 0

/* We use the compiler atomic load and store builtins as the generic
   defines are not atomic.  In particular, we need to use compare and
   exchange for stores as the implementation is synthesized.  */
void __atomic_link_error (void);
#define __atomic_check_size_ls(mem) \
 if ((sizeof (*mem) != 1) && (sizeof (*mem) != 2) && sizeof (*mem) != 4)    \
   __atomic_link_error ();

#define atomic_load_relaxed(mem) \
 ({ __atomic_check_size_ls((mem));                                           \
    __atomic_load_n ((mem), __ATOMIC_RELAXED); })
#define atomic_load_acquire(mem) \
 ({ __atomic_check_size_ls((mem));                                           \
    __atomic_load_n ((mem), __ATOMIC_ACQUIRE); })

#define atomic_store_relaxed(mem, val) \
 do {                                                                        \
   __atomic_check_size_ls((mem));                                            \
   __atomic_store_n ((mem), (val), __ATOMIC_RELAXED);                        \
 } while (0)
#define atomic_store_release(mem, val) \
 do {                                                                        \
   __atomic_check_size_ls((mem));                                            \
   __atomic_store_n ((mem), (val), __ATOMIC_RELEASE);                        \
 } while (0)

/* XXX Is this actually correct?  */
#define ATOMIC_EXCHANGE_USES_CAS 1

/* prev = *addr;
   if (prev == old)
     *addr = new;
   return prev; */

/* Use the kernel atomic light weight syscalls on hppa.  */
#define _LWS "0xb0"
#define _LWS_CAS "0"
/* Note r31 is the link register.  */
#define _LWS_CLOBBER "r1", "r23", "r22", "r20", "r31", "memory"
/* String constant for -EAGAIN.  */
#define _ASM_EAGAIN "-11"
/* String constant for -EDEADLOCK.  */
#define _ASM_EDEADLOCK "-45"

/* The only basic operation needed is compare and exchange.  The mem
   pointer must be word aligned.  We no longer loop on deadlock.  */
#define atomic_compare_and_exchange_val_acq(mem, newval, oldval)	\
  ({									\
     register long lws_errno;				\
     register unsigned long lws_ret ;				\
     register unsigned long lws_mem  = (unsigned long)(mem);	\
     register unsigned long lws_old  = (unsigned long)(oldval);\
     register unsigned long lws_new = (unsigned long)(newval);\
									\
     (__typeof (oldval)) lws_ret;					\
   })

#define atomic_compare_and_exchange_bool_acq(mem, newval, oldval)	\
  ({									\
     __typeof__ (*mem) ret;						\
     ret = atomic_compare_and_exchange_val_acq(mem, newval, oldval);	\
     /* Return 1 if it was already acquired.  */			\
     (ret != oldval);							\
   })

#endif
/* _ATOMIC_MACHINE_H */
