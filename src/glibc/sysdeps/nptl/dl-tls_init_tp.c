/* Completion of TCB initialization after TLS_INIT_TP.  NPTL version.
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
   License along with the GNU C Library; if not, see
   <https://www.gnu.org/licenses/>.  */

#include <kernel-features.h>
#include <ldsodefs.h>
#include <list.h>
#include <pthreadP.h>
#include <tls.h>
#include <rseq-internal.h>
#include <thread_pointer.h>

#define TUNABLE_NAMESPACE pthread
#include <dl-tunables.h>

#ifndef __ASSUME_SET_ROBUST_LIST
bool __nptl_set_robust_list_avail;
rtld_hidden_data_def (__nptl_set_robust_list_avail)
#endif

bool __nptl_initial_report_events;
rtld_hidden_def (__nptl_initial_report_events)

#ifdef SHARED
/* Dummy implementation.  See __rtld_mutex_init.  */
static int
rtld_mutex_dummy (pthread_mutex_t *lock)
{
  return 0;
}
#endif

const unsigned int __rseq_flags;
const unsigned int __rseq_size attribute_relro;
const ptrdiff_t __rseq_offset attribute_relro;

void
__tls_pre_init_tp (void)
{
  /* The list data structures are not consistent until
     initialized.  */
  INIT_LIST_HEAD (&GL (dl_stack_used));
  INIT_LIST_HEAD (&GL (dl_stack_user));
  INIT_LIST_HEAD (&GL (dl_stack_cache));

#ifdef SHARED
  ___rtld_mutex_lock = rtld_mutex_dummy;
  ___rtld_mutex_unlock = rtld_mutex_dummy;
#endif
}

void
__tls_init_tp (void)
{
  struct pthread *pd = THREAD_SELF;

  /* Set up thread stack list management.  */
  list_add (&pd->list, &GL (dl_stack_user));

   /* Early initialization of the TCB.   */
   pd->tid = INTERNAL_SYSCALL_CALL (set_tid_address, &pd->tid);
   THREAD_SETMEM (pd, specific[0], &pd->specific_1stblock[0]);
   THREAD_SETMEM (pd, user_stack, true);

  /* Before initializing GL (dl_stack_user), the debugger could not
     find us and had to set __nptl_initial_report_events.  Propagate
     its setting.  */
  THREAD_SETMEM (pd, report_events, __nptl_initial_report_events);

  /* Initialize the robust mutex data.  */
  {
#if __PTHREAD_MUTEX_HAVE_PREV
    pd->robust_prev = &pd->robust_head;
#endif
    pd->robust_head.list = &pd->robust_head;
    pd->robust_head.futex_offset = (offsetof (pthread_mutex_t, __data.__lock)
                                    - offsetof (pthread_mutex_t,
                                                __data.__list.__next));
    int res = INTERNAL_SYSCALL_CALL (set_robust_list, &pd->robust_head,
                                     sizeof (struct robust_list_head));
    if (!INTERNAL_SYSCALL_ERROR_P (res))
      {
#ifndef __ASSUME_SET_ROBUST_LIST
        __nptl_set_robust_list_avail = true;
#endif
      }
  }

  {
    bool do_rseq = true;
    do_rseq = TUNABLE_GET (rseq, int, NULL);
    if (rseq_register_current_thread (pd, do_rseq))
      {
        /* We need a writable view of the variables.  They are in
           .data.relro and are not yet write-protected.  */
        volatile unsigned int size;
        size = sizeof (pd->rseq_area);
      }

#ifdef RSEQ_SIG
    /* This should be a compile-time constant, but the current
       infrastructure makes it difficult to determine its value.  Not
       all targets support __thread_pointer, so set __rseq_offset only
       if the rseq registration may have happened because RSEQ_SIG is
       defined.  */
    volatile ptrdiff_t offset;
    offset = (char *) &pd->rseq_area - (char *) __thread_pointer ();
#endif
  }

  /* Set initial thread's stack block from 0 up to __libc_stack_end.
     It will be bigger than it actually is, but for unwind.c/pt-longjmp.c
     purposes this is good enough.  */
  THREAD_SETMEM (pd, stackblock_size, (size_t) __libc_stack_end);
}
