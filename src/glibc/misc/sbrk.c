/* Copyright (C) 1991-2024 Free Software Foundation, Inc.
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

/* Mark symbols hidden in static PIE for early self relocation to work.  */
#if BUILD_PIE_DEFAULT
# pragma GCC visibility push(hidden)
#endif
#include <errno.h>
#include <libc-internal.h>
#include <stdbool.h>
#include <stdint.h>
#include <unistd.h>
#include <stdio.h>
#include <syscall-template.h>

/* Defined in brk.c.  */
// This is the "virtual brk" exposed to the caller
// while the actual end of LinearMemory might be a
// higher address aligned to pages
extern void *__curbrk;
// extern int __brk (void *addr);

/* Extend the process's data space by INCREMENT.
   If INCREMENT is negative, shrink data space by - INCREMENT.
   Return start of new space allocated, or -1 for errors.  */

#define PAGESIZE (0x10000)

void *
__sbrk (intptr_t increment)
{
	return MAKE_SYSCALL(176, "syscall|sbrk", (uint64_t) increment, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
    // __curbrk = __builtin_wasm_memory_size(0) * PAGESIZE;
    
    // // sbrk(0) returns the current memory size.
    // if (increment == 0) {
    //     // The wasm spec doesn't guarantee that memory.grow of 0 always succeeds.
    //     return __curbrk;
    // }

    // // FIXME: now two threads calling this sbrk simultaneously
    // // will lead to the corruption of __curbrk, so we should move
    // // this implementation into the runtime, and protect the __curbrk
    // // with mutex (i.e.  preventing two sbrk to be executed at the same time)

    // void * linear_mem_end = __builtin_wasm_memory_size(0) * PAGESIZE;
    // void * old_break = __curbrk;
    // void * new_break = old_break + increment;

    // if (new_break <= linear_mem_end) {
    //     // In this case, we don't need to grow linear mem
    //     __curbrk = new_break;
    //     return old_break;
    // }

    // // Now we need to grow linear mem
    // // int new_pages = (new_break - linear_mem_end) / PAGESIZE;
    // int new_pages = (new_break - linear_mem_end + PAGESIZE - 1) / PAGESIZE;

	// MAKE_SYSCALL(176, "syscall|sbrk", (uint64_t) increment, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
    // if (__builtin_wasm_memory_grow(0, new_pages) < 0) {
    //     errno = ENOMEM;
    //     return (void *)-1;
    // }

    // __curbrk = new_break;
    // return old_break;
}

// void *
// __sbrk (intptr_t increment)
// {
//   /* Controls whether __brk (0) is called to read the brk value from
//      the kernel.  */
//   bool update_brk = __curbrk == NULL;

// #if defined (SHARED) && ! IS_IN (rtld)
//   if (!__libc_initial)
//     {
//       if (increment != 0)
// 	{
// 	  /* Do not allow changing the brk from an inner libc because
// 	     it cannot be synchronized with the outer libc's brk.  */
// 	  __set_errno (ENOMEM);
// 	  return (void *) -1;
// 	}
//       /* Querying the kernel's brk value from an inner namespace is
// 	 fine.  */
//       update_brk = true;
//     }
// #endif

//   if (update_brk)
//     if (__brk (0) < 0)		/* Initialize the break.  */
//       return (void *) -1;

//   if (increment == 0)
//     return __curbrk;

//   void *oldbrk = __curbrk;
//   if (increment > 0
//       ? ((uintptr_t) oldbrk + (uintptr_t) increment < (uintptr_t) oldbrk)
//       : ((uintptr_t) oldbrk < (uintptr_t) -increment))
//     {
//       __set_errno (ENOMEM);
//       return (void *) -1;
//     }

//   if (__brk (oldbrk + increment) < 0)
//     return (void *) -1;

//   return oldbrk;
// }

libc_hidden_def (__sbrk)
weak_alias (__sbrk, sbrk)
