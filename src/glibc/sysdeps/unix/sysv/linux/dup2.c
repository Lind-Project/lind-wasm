/* Duplicate a file descriptor.  Linux version.  */
/* Copyright (C) 2011–2024 Free Software Foundation, Inc. */

#include <fcntl.h>
#include <unistd.h>
#include <sysdep.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

/* Duplicate FD to FD2, closing the old FD2 and making FD2 be
   open the same file as FD is.  Return FD2 or -1.  */
int
__dup2 (int fd, int fd2)
{
  return MAKE_SYSCALL(DUP2_SYSCALL, "syscall|dup2",
                      (uint64_t) fd, (uint64_t) fd2,
                      NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

libc_hidden_def (__dup2)
weak_alias (__dup2, dup2)
