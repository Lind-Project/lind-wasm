/* Read a directory.  Linux no-LFS version.
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

#include <dirent.h>

#if !_DIRENT_MATCHES_DIRENT64
#include <dirstream.h>

/* Read a directory entry from DIRP.  */
struct dirent *
__readdir_unlocked (DIR *dirp)
{
  struct dirent64 *src;
  struct dirent *dst;
  int saved_errno = errno;

  if (dirp->offset >= dirp->size)
    {
      /* We have exhausted the current buffer. Refill it.  */

      size_t maxread = dirp->allocation;
      ssize_t bytes;

      bytes = __getdents (dirp->fd, dirp->data, maxread);

      if (bytes <= 0)
        {
          /* Linux may fail with ENOENT on some file systems if the
             directory inode is marked as dead (deleted). POSIX treats
             this as a regular end-of-directory condition, so do not
             set errno in that case, to indicate success.  */
          if (bytes == 0 || errno == ENOENT)
            __set_errno (saved_errno);
          return NULL;
        }
      dirp->size = (size_t) bytes;

      /* Reset the offset into the buffer.  */
      dirp->offset = 0;
    }


  src = (struct dirent64 *) &dirp->data[dirp->offset];
  dst = (struct dirent *) &dirp->data[dirp->offset];

  /* Copy source fields into local variables first, so that in-place
     rewriting does not interfere with reading the original record.  */
  /*TODO: directing casting 64bit field to 32bit field is not safe and could cause potential problems. 
     It is rare to have d_ino and d_off to be large enough to cause the problem but should still might need to be fixed in the future*/
  __ino64_t src_ino = src->d_ino;
  __off64_t src_off = src->d_off;
  unsigned short src_reclen = src->d_reclen;
  unsigned char src_type = src->d_type;
  char src_name[256];

  snprintf(src_name, sizeof(src_name), "%s", src->d_name);

  /* Convert the dirent64 record into a plain dirent record.  */
  dst->d_ino = (__ino_t) src_ino;
  dst->d_off = (__off_t) src_off;
  dst->d_reclen = src_reclen;
  dst->d_type = src_type;
  snprintf(dst->d_name, sizeof(dst->d_name), "%s", src_name);

  dirp->offset += src_reclen;
  dirp->filepos = src_off;

  return dst;
}

struct dirent *
__readdir (DIR *dirp)
{
  struct dirent *dp;

#if IS_IN (libc)
  __libc_lock_lock (dirp->lock);
#endif
  dp = __readdir_unlocked (dirp);
#if IS_IN (libc)
  __libc_lock_unlock (dirp->lock);
#endif

  return dp;
}
weak_alias (__readdir, readdir)

#endif
