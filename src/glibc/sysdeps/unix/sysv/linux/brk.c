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
#include <lind_syscall_num.h>

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
   /* Keep the raw return in an int and check it BEFORE publishing it as the
      new break.

      With TRANSLATE_ERRNO_ON the wrapper already set errno and returned -1
      when the syscall failed (brk_syscall returns -ENOMEM when the requested
      growth is not available).  Assigning that -1 straight into __curbrk
      stored 0xffffffff on wasm32, and since pointer comparison is unsigned
      the `__curbrk < addr` guard below then read as SUCCESS: __brk returned
      0, __sbrk handed malloc a break it had never been granted, and the
      first write to it faulted on memory that is still PROT_NONE.  __curbrk
      was also left permanently corrupted, poisoning every later sbrk().

      A successful brk_syscall returns a page-aligned address, so it can
      never itself be -1 -- the same aliasing argument make_threei_call
      already relies on for mmap.  */
   int ret = MAKE_LEGACY_SYSCALL(BRK_SYSCALL, "syscall|brk", (uint64_t) addr, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
   if (ret == -1)
      return -1;   /* errno already set by the translation layer */

   __curbrk = (void *) ret;
   if (__curbrk < addr)
   {
      __set_errno (ENOMEM);
      return -1;
   }

   return 0;
}
weak_alias (__brk, brk)
