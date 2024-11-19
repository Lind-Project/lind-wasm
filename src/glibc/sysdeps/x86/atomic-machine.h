/* Atomic operations.  X86 version.
   Copyright (C) 2018-2024 Free Software Foundation, Inc.
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
   License along with the GNU C Library; if not, see
   <https://www.gnu.org/licenses/>.  */

#ifndef _X86_ATOMIC_MACHINE_H
#define _X86_ATOMIC_MACHINE_H 1

#include <stdint.h>
#include <tls.h>			/* For tcbhead_t.  */
#include <libc-pointer-arith.h>		/* For cast_to_integer.  */

#define LOCK_PREFIX "lock;"

#define USE_ATOMIC_COMPILER_BUILTINS	1

#ifdef __x86_64__
# define __HAVE_64B_ATOMICS		1
# define SP_REG				"rsp"
# define SEG_REG			"fs"
# define BR_CONSTRAINT			"q"
# define IBR_CONSTRAINT			"iq"
#else
/* Since the Pentium, i386 CPUs have supported 64-bit atomics, but the
   i386 psABI supplement provides only 4-byte alignment for uint64_t
   inside structs, so it is currently not possible to use 64-bit
   atomics on this platform.  */
# define __HAVE_64B_ATOMICS		0
# define SP_REG				"esp"
# define SEG_REG			"gs"
# define BR_CONSTRAINT			"r"
# define IBR_CONSTRAINT			"ir"
#endif
#define ATOMIC_EXCHANGE_USES_CAS	0

#define atomic_compare_and_exchange_val_acq(mem, newval, oldval) \
  __sync_val_compare_and_swap (mem, oldval, newval)
#define atomic_compare_and_exchange_bool_acq(mem, newval, oldval) \
  (! __sync_bool_compare_and_swap (mem, oldval, newval))


#define __arch_c_compare_and_exchange_val_8_acq(mem, newval, oldval) \
  ({ __typeof (*mem) ret;						      \
     ret; })

#define __arch_c_compare_and_exchange_val_16_acq(mem, newval, oldval) \
  ({ __typeof (*mem) ret;						      \
     ret; })

#define __arch_c_compare_and_exchange_val_32_acq(mem, newval, oldval) \
  ({ __typeof (*mem) ret;						      \
     ret; })

#ifdef __x86_64__
# define __arch_c_compare_and_exchange_val_64_acq(mem, newval, oldval) \
  ({ __typeof (*mem) ret;						      \
     ret; })
# define do_exchange_and_add_val_64_acq(pfx, mem, value) 0
# define do_add_val_64_acq(pfx, mem, value) do { } while (0)
#else
/* XXX We do not really need 64-bit compare-and-exchange.  At least
   not in the moment.  Using it would mean causing portability
   problems since not many other 32-bit architectures have support for
   such an operation.  So don't define any code for now.  If it is
   really going to be used the code below can be used on Intel Pentium
   and later, but NOT on i486.  */
# define __arch_c_compare_and_exchange_val_64_acq(mem, newval, oldval) \
  ({ __typeof (*mem) ret = *(mem);					      \
     __atomic_link_error ();						      \
     ret = (newval);							      \
     ret = (oldval);							      \
     ret; })

# define __arch_compare_and_exchange_val_64_acq(mem, newval, oldval)	      \
  ({ __typeof (*mem) ret = *(mem);					      \
     __atomic_link_error ();						      \
     ret = (newval);							      \
     ret = (oldval);							      \
     ret; })

# define do_exchange_and_add_val_64_acq(pfx, mem, value) \
  ({ __typeof (value) __addval = (value);				      \
     __typeof (*mem) __result;						      \
     __typeof (mem) __memp = (mem);					      \
     __typeof (*mem) __tmpval;						      \
     __result = *__memp;						      \
     do									      \
       __tmpval = __result;						      \
     while ((__result = pfx##_compare_and_exchange_val_64_acq		      \
	     (__memp, __result + __addval, __result)) == __tmpval);	      \
     __result; })

# define do_add_val_64_acq(pfx, mem, value) \
  {									      \
    __typeof (value) __addval = (value);				      \
    __typeof (mem) __memp = (mem);					      \
    __typeof (*mem) __oldval = *__memp;					      \
    __typeof (*mem) __tmpval;						      \
    do									      \
      __tmpval = __oldval;						      \
    while ((__oldval = pfx##_compare_and_exchange_val_64_acq		      \
	    (__memp, __oldval + __addval, __oldval)) == __tmpval);	      \
  }
#endif


/* Note that we need no lock prefix.  */
#define atomic_exchange_acq(mem, newvalue) \
  ({ __typeof (*mem) result;						      \
     if (sizeof (*mem) == 1)						      \
      {}					      \
     else if (sizeof (*mem) == 2)					      \
      {}					      \
     else if (sizeof (*mem) == 4)					      \
      {}					      \
     else if (__HAVE_64B_ATOMICS)					      \
      {}					      \
     else								      \
       {								      \
	 result = 0;							      \
	 __atomic_link_error ();					      \
       }								      \
     result; })


#define __arch_exchange_and_add_body(lock, pfx, mem, value) \
  ({ __typeof (*mem) __result;						      \
     __typeof (value) __addval = (value);				      \
     if (sizeof (*mem) == 1)						      \
      {}					      \
     else if (sizeof (*mem) == 2)					      \
      {}					      \
     else if (sizeof (*mem) == 4)					      \
      {}					      \
     else if (__HAVE_64B_ATOMICS)					      \
      {}					      \
     else								      \
       __result = do_exchange_and_add_val_64_acq (pfx, (mem), __addval);      \
     __result; })

#define atomic_exchange_and_add(mem, value) \
  __sync_fetch_and_add (mem, value)

#define __arch_exchange_and_add_cprefix \
  "cmpl $0, %%" SEG_REG ":%P4\n\tje 0f\n\tlock\n0:\t"

#define catomic_exchange_and_add(mem, value) \
  __arch_exchange_and_add_body (__arch_exchange_and_add_cprefix, __arch_c,    \
				mem, value)


#define __arch_add_body(lock, pfx, apfx, mem, value) \
  do {									      \
    if (__builtin_constant_p (value) && (value) == 1)			      \
      pfx##_increment (mem);						      \
    else if (__builtin_constant_p (value) && (value) == -1)		      \
      pfx##_decrement (mem);						      \
    else if (sizeof (*mem) == 1)					      \
      {}					      \
    else if (sizeof (*mem) == 2)					      \
      {}					      \
    else if (sizeof (*mem) == 4)					      \
      {}					      \
    else if (__HAVE_64B_ATOMICS)					      \
      {}					      \
    else								      \
      do_add_val_64_acq (apfx, (mem), (value));				      \
  } while (0)

# define atomic_add(mem, value) \
  __arch_add_body (LOCK_PREFIX, atomic, __arch, mem, value)

#define __arch_add_cprefix \
  "cmpl $0, %%" SEG_REG ":%P3\n\tje 0f\n\tlock\n0:\t"

#define catomic_add(mem, value) \
  __arch_add_body (__arch_add_cprefix, atomic, __arch_c, mem, value)


#define atomic_add_negative(mem, value) \
  ({ unsigned char __result;						      \
     if (sizeof (*mem) == 1)						      \
       __atomic_link_error ();						      \
     else if (sizeof (*mem) == 2)					      \
       __atomic_link_error ();						      \
     else if (sizeof (*mem) == 4)					      \
       __atomic_link_error ();						      \
     else if (__HAVE_64B_ATOMICS)					      \
       __atomic_link_error ();						      \
     else								      \
       __atomic_link_error ();						      \
     __result; })


#define atomic_add_zero(mem, value) \
  ({ unsigned char __result;						      \
     if (sizeof (*mem) == 1)						      \
      __atomic_link_error ();					      \
     else if (sizeof (*mem) == 2)					      \
      __atomic_link_error ();					      \
     else if (sizeof (*mem) == 4)					      \
      __atomic_link_error ();					      \
     else if (__HAVE_64B_ATOMICS)					      \
      __atomic_link_error ();					      \
     else								      \
       __atomic_link_error ();					      \
     __result; })


#define __arch_increment_body(lock, pfx, mem) \
  do {									      \
    if (sizeof (*mem) == 1)						      \
       {}					      \
    else if (sizeof (*mem) == 2)					      \
       {}					      \
    else if (sizeof (*mem) == 4)					      \
       {}					      \
    else if (__HAVE_64B_ATOMICS)					      \
       {}					      \
    else								      \
      do_add_val_64_acq (pfx, mem, 1);					      \
  } while (0)

#define atomic_increment(mem) __arch_increment_body (LOCK_PREFIX, __arch, mem)

#define __arch_increment_cprefix \
  "cmpl $0, %%" SEG_REG ":%P2\n\tje 0f\n\tlock\n0:\t"

#define catomic_increment(mem) \
  __arch_increment_body (__arch_increment_cprefix, __arch_c, mem)


#define atomic_increment_and_test(mem) \
  ({ unsigned char __result;						      \
     if (sizeof (*mem) == 1)						      \
      __atomic_link_error ();					      \
     else if (sizeof (*mem) == 2)					      \
      __atomic_link_error ();					      \
     else if (sizeof (*mem) == 4)					      \
      __atomic_link_error ();					      \
     else if (__HAVE_64B_ATOMICS)					      \
      __atomic_link_error ();					      \
     else								      \
       __atomic_link_error ();					      \
     __result; })


#define __arch_decrement_body(lock, pfx, mem) \
  do {									      \
    if (sizeof (*mem) == 1)						      \
      {}					      \
    else if (sizeof (*mem) == 2)					      \
      {}					      \
    else if (sizeof (*mem) == 4)					      \
      {}					      \
    else if (__HAVE_64B_ATOMICS)					      \
      {}					      \
    else								      \
      do_add_val_64_acq (pfx, mem, -1);					      \
  } while (0)

#define atomic_decrement(mem) __arch_decrement_body (LOCK_PREFIX, __arch, mem)

#define __arch_decrement_cprefix \
  "cmpl $0, %%" SEG_REG ":%P2\n\tje 0f\n\tlock\n0:\t"

#define catomic_decrement(mem) \
  __arch_decrement_body (__arch_decrement_cprefix, __arch_c, mem)


#define atomic_decrement_and_test(mem) \
  ({ unsigned char __result;						      \
     if (sizeof (*mem) == 1)						      \
      __result;                                \
     else if (sizeof (*mem) == 2)					      \
      __result;                                 \
     else if (sizeof (*mem) == 4)					      \
      __result;                                 \
     else								      \
     __result; })


#define atomic_bit_set(mem, bit) \
  do {									      \
    if (sizeof (*mem) == 1)						      \
      __atomic_link_error ();						      \
    else if (sizeof (*mem) == 2)					      \
      __atomic_link_error ();						      \
    else if (sizeof (*mem) == 4)					      \
      __atomic_link_error ();						      \
    else if (__builtin_constant_p (bit) && (bit) < 32)			      \
      __atomic_link_error ();						      \
    else if (__HAVE_64B_ATOMICS)					      \
      __atomic_link_error ();						      \
    else								      \
      __atomic_link_error ();						      \
  } while (0)


#define atomic_bit_test_set(mem, bit) \
  ({ unsigned char __result;						      \
     if (sizeof (*mem) == 1)						      \
     else if (sizeof (*mem) == 2)					      \
     else if (sizeof (*mem) == 4)					      \
     else if (__HAVE_64B_ATOMICS)					      \
     else							      	      \
       __atomic_link_error ();					      \
     __result; })


#define __arch_and_body(lock, mem, mask) \
  do {									      \
    if (sizeof (*mem) == 1)						      \
      __atomic_link_error ();						      \
    else if (sizeof (*mem) == 2)					      \
      __atomic_link_error ();						      \
    else if (sizeof (*mem) == 4)					      \
      __atomic_link_error ();						      \
    else if (__HAVE_64B_ATOMICS)					      \
      __atomic_link_error ();						      \
    else								      \
      __atomic_link_error ();						      \
  } while (0)

#define __arch_cprefix \
  "cmpl $0, %%" SEG_REG ":%P3\n\tje 0f\n\tlock\n0:\t"

#define atomic_and(mem, mask) __arch_and_body (LOCK_PREFIX, mem, mask)

#define catomic_and(mem, mask) __arch_and_body (__arch_cprefix, mem, mask)


#define __arch_or_body(lock, mem, mask) \
  do {									      \
    if (sizeof (*mem) == 1)						      \
      __atomic_link_error ();						      \
    else if (sizeof (*mem) == 2)					      \
      __atomic_link_error ();						      \
    else if (sizeof (*mem) == 4)					      \
      __atomic_link_error ();						      \
    else if (__HAVE_64B_ATOMICS)					      \
      __atomic_link_error ();						      \
    else								      \
      __atomic_link_error ();						      \
  } while (0)

#define atomic_or(mem, mask) __arch_or_body (LOCK_PREFIX, mem, mask)

#define catomic_or(mem, mask) __arch_or_body (__arch_cprefix, mem, mask)

/* We don't use mfence because it is supposedly slower due to having to
   provide stronger guarantees (e.g., regarding self-modifying code).  */
#define atomic_full_barrier()
#define atomic_read_barrier() 
#define atomic_write_barrier() 

#define atomic_spin_nop() 

#endif /* atomic-machine.h */
