/* Get directory entries.  Linux non-LFS version.
   Copyright (C) 1993-2024 Free Software Foundation, Inc.
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

#include <dirent.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

#if !_DIRENT_MATCHES_DIRENT64

# include <unistd.h>
# include <string.h>
# include <errno.h>


# ifndef DIRENT_SET_DP_INO
#  define DIRENT_SET_DP_INO(dp, value) (dp)->d_ino = (value)
# endif

/* Pack the dirent64 struct down into 32-bit offset/inode fields, and
   ensure that no overflow occurs.  */
ssize_t
__getdents (int fd, void *buf0, size_t nbytes)
{
   uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf0);
   
   return MAKE_SYSCALL(GETDENTS_SYSCALL, "syscall|getdents", (uint64_t) fd, host_buf, (uint64_t) nbytes, NOTUSED, NOTUSED, NOTUSED);
}

# undef DIRENT_SET_DP_INO

#endif /* _DIRENT_MATCHES_DIRENT64  */
