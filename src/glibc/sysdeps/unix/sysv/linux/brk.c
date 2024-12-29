/* Change data segment.  Linux generic version.
   Copyright (C) 2020-2024 Free Software Foundation, Inc.
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

#include <errno.h>
#include <unistd.h>
#include <sysdep.h>
#include <brk_call.h>
#include <syscall-template.h>

/* This must be initialized data because commons can't have aliases.  */
// This is the "virtual brk" exposed to the caller
// while the actual end of LinearMemory might be a
// higher address aligned to pages
void *__curbrk = 0;

#if HAVE_INTERNAL_BRK_ADDR_SYMBOL
/* Old braindamage in GCC's crtstuff.c requires this symbol in an attempt
   to work around different old braindamage in the old Linux ELF dynamic
   linker.  */
weak_alias (__curbrk, ___brk_addr)
#endif

#define PAGESIZE (0x10000)

int
__brk (void *addr)
{
	return MAKE_SYSCALL(175, "syscall|brk", (uint64_t) addr, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
//   __curbrk = __builtin_wasm_memory_size(0) * PAGESIZE;

//     // FIXME: now two threads calling this sbrk simultaneously
//     // will lead to the corruption of __curbrk, so we should move
//     // this implementation into the runtime, and protect the __curbrk
//     // with mutex (i.e.  preventing two brk/sbrk to be executed at the same time)

//     void * linear_mem_end = __builtin_wasm_memory_size(0) * PAGESIZE;
//     void * old_break = __curbrk;
//     void * new_break = addr;

//     if (new_break <= linear_mem_end) {
//         // In this case, we don't need to grow linear mem
//         __curbrk = new_break;
//         return old_break;
//     }

//     // Now we need to grow linear mem
//     int new_pages = (new_break - linear_mem_end) / PAGESIZE;

//     if (__builtin_wasm_memory_grow(0, new_pages) < 0) {
//         errno = ENOMEM;
//         return (void *)-1;
//     }

//     __curbrk = new_break;
//     return old_break;
}
weak_alias (__brk, brk)
