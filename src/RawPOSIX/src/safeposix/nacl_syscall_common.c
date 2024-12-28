/*
 * Copyright (c) 2012 The Native Client Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

/*
 * NaCl service run-time, non-platform specific system call helper routines.
 */

#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>

/* avoid errors caused by conflicts with feature_test_macros(7) */
#undef _POSIX_C_SOURCE
#undef _XOPEN_SOURCE
#include <stdio.h>
#include <Python.h>
#include <string.h>
#include <unistd.h>
#include <time.h>
#include <fcntl.h>
#include <netinet/in.h>

#include "native_client/src/trusted/service_runtime/nacl_syscall_common.h"

#include "native_client/src/include/nacl_assert.h"
#include "native_client/src/include/nacl_macros.h"
#include "native_client/src/include/nacl_platform.h"
#include "native_client/src/include/portability_process.h"
#include "native_client/src/include/portability_string.h"

#include "native_client/src/shared/platform/nacl_check.h"
#include "native_client/src/shared/platform/nacl_clock.h"
#include "native_client/src/shared/platform/nacl_exit.h"
#include "native_client/src/shared/platform/nacl_host_desc.h"
#include "native_client/src/shared/platform/nacl_host_dir.h"
#include "native_client/src/shared/platform/nacl_log.h"
#include "native_client/src/shared/platform/nacl_sync_checked.h"
#include "native_client/src/shared/platform/nacl_time.h"

#include "native_client/src/trusted/desc/nacl_desc_base.h"
#include "native_client/src/trusted/desc/nacl_desc_cond.h"
#include "native_client/src/trusted/desc/nacl_desc_dir.h"
#include "native_client/src/trusted/desc/nacl_desc_effector_trusted_mem.h"
#include "native_client/src/trusted/desc/nacl_desc_imc.h"
#include "native_client/src/trusted/desc/nacl_desc_imc_shm.h"
#include "native_client/src/trusted/desc/nacl_desc_invalid.h"
#include "native_client/src/trusted/desc/nacl_desc_io.h"
#include "native_client/src/trusted/desc/nacl_desc_mutex.h"
#include "native_client/src/trusted/desc/nacl_desc_semaphore.h"
#include "native_client/src/trusted/desc/nrd_xfer.h"

#include "native_client/src/trusted/fault_injection/fault_injection.h"

#include "native_client/src/trusted/service_runtime/include/bits/mman.h"
#include "native_client/src/trusted/service_runtime/include/bits/nacl_syscalls.h"
#include "native_client/src/trusted/service_runtime/include/sys/errno.h"
#include "native_client/src/trusted/service_runtime/include/sys/fcntl.h"
#include "native_client/src/trusted/service_runtime/include/sys/stat.h"
#include "native_client/src/trusted/service_runtime/include/sys/uio.h"
#include "native_client/src/trusted/service_runtime/include/sys/sigaction.h"

#include "native_client/src/trusted/service_runtime/include/sys/nacl_test_crash.h"
#include "native_client/src/trusted/service_runtime/internal_errno.h"

#include "native_client/src/trusted/service_runtime/nacl_app_thread.h"
#include "native_client/src/trusted/service_runtime/nacl_copy.h"
#include "native_client/src/trusted/service_runtime/nacl_globals.h"
#include "native_client/src/trusted/service_runtime/nacl_signal.h"
#include "native_client/src/trusted/service_runtime/nacl_switch_to_app.h"
#include "native_client/src/trusted/service_runtime/nacl_syscall_handlers.h"
#include "native_client/src/trusted/service_runtime/nacl_text.h"
#include "native_client/src/trusted/service_runtime/nacl_thread_nice.h"
#include "native_client/src/trusted/service_runtime/nacl_tls.h"
#include "native_client/src/trusted/service_runtime/sel_ldr.h"
#include "native_client/src/trusted/service_runtime/sel_memory.h"
#include "native_client/src/trusted/service_runtime/thread_suspension.h"
#include "native_client/src/trusted/service_runtime/nacl_syscall_strace.h"
#include "native_client/src/trusted/service_runtime/win/debug_exception_handler.h"

#if NACL_WINDOWS
#  include "native_client/src/trusted/service_runtime/win/debug_exception_handler.h"
#  include "native_client/src/shared/platform/win/xlate_system_error.h"
#endif

#include "native_client/src/trusted/validator/ncvalidate.h"
#include "native_client/src/trusted/validator/validation_metadata.h"
#include "native_client/src/trusted/service_runtime/env_cleanser.h"
#include "native_client/src/trusted/service_runtime/nacl_all_modules.h"
#include "native_client/src/trusted/service_runtime/nacl_app.h"
#include "native_client/src/trusted/service_runtime/load_file.h"

#define kKnownInvalidDescNumber (-1)
#define kdefault_io_buffer_bytes_to_log 64ull
#define kMaxUsableFileSize (SIZE_MAX >> 1)
#define MIN(a, b) ((size_t)((a < b) ? a : b))

struct NaClDescQuotaInterface;
struct NaClSyscallTableEntry nacl_syscall[NACL_MAX_SYSCALLS];
long long gettimens() {
    struct timespec tp;
    clock_gettime(CLOCK_MONOTONIC, &tp);
    return (long long)tp.tv_sec * 1000000000LL + tp.tv_nsec;
}

int32_t NaClSysNotImplementedDecoder(struct NaClAppThread *natp) {
  return -NACL_ABI_ENOSYS;
}


void NaClAddSyscall(int num, int32_t (*fn)(struct NaClAppThread *)) {
  if (nacl_syscall[num].handler != &NaClSysNotImplementedDecoder) {
    NaClLog(LOG_FATAL, "Duplicate syscall number %d\n", num);
  }
  nacl_syscall[num].handler = fn;
}

int32_t NaClSysNull(struct NaClAppThread *natp) {
  UNREFERENCED_PARAMETER(natp);
  return 0;
}

int32_t NaClSysBrk(struct NaClAppThread *natp,
                   uintptr_t            new_break) {
  struct NaClApp        *nap = natp->nap;
  uintptr_t             break_addr;
  int32_t               rv = -NACL_ABI_EINVAL;
  struct NaClVmmapIter  iter;
  struct NaClVmmapEntry *ent;
  struct NaClVmmapEntry *next_ent;
  uintptr_t             sys_break;
  uintptr_t             sys_new_break;
  uintptr_t             usr_last_data_page;
  uintptr_t             usr_new_last_data_page;
  uintptr_t             last_internal_data_addr;
  uintptr_t             last_internal_page;
  uintptr_t             start_new_region;
  uintptr_t             region_size;
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  break_addr = nap->break_addr;

  NaClLog(3, "Entered NaClSysBrk(new_break 0x%08"NACL_PRIxPTR")\n",
          new_break);

  sys_new_break = NaClUserToSysAddr(nap, new_break);
  NaClLog(3, "sys_new_break 0x%08"NACL_PRIxPTR"\n", sys_new_break);

  if (kNaClBadAddress == sys_new_break) {
    goto cleanup_no_lock;
  }

  if (NACL_SYNC_OK != NaClMutexLock(&nap->mu)) {
    NaClLog(LOG_ERROR, "Could not get app lock for 0x%08"NACL_PRIxPTR"\n",
            (uintptr_t) nap);
    goto cleanup_no_lock;
  }
  if (new_break < nap->data_end) {
    NaClLog(4, "new_break before data_end (0x%"NACL_PRIxPTR")\n",
            nap->data_end);
    goto cleanup;
  }
  if (new_break <= nap->break_addr) {
    /* freeing memory */
    NaClLog(4, "new_break before break (0x%"NACL_PRIxPTR"); freeing\n",
            nap->break_addr);
    nap->break_addr = new_break;
    break_addr = new_break;
  } else {
    /*
     * See if page containing new_break is in mem_map; if so, we are
     * essentially done -- just update break_addr.  Otherwise, we
     * extend the VM map entry from the page containing the current
     * break to the page containing new_break.
     */

    sys_break = NaClUserToSys(nap, nap->break_addr);

    usr_last_data_page = (nap->break_addr - 1) >> NACL_PAGESHIFT;

    usr_new_last_data_page = (new_break - 1) >> NACL_PAGESHIFT;

    last_internal_data_addr = NaClRoundAllocPage(new_break) - 1;
    last_internal_page = last_internal_data_addr >> NACL_PAGESHIFT;

    NaClLog(4, ("current break sys addr 0x%08"NACL_PRIxPTR", "
                "usr last data page 0x%"NACL_PRIxPTR"\n"),
            sys_break, usr_last_data_page);
    NaClLog(4, "new break usr last data page 0x%"NACL_PRIxPTR"\n",
            usr_new_last_data_page);
    NaClLog(4, "last internal data addr 0x%08"NACL_PRIxPTR"\n",
            last_internal_data_addr);

    if (!NaClVmmapFindPageIter(&nap->mem_map, usr_last_data_page, &iter)
        || NaClVmmapIterAtEnd(&iter)) {
      NaClLog(LOG_FATAL, ("current break (0x%08"NACL_PRIxPTR", "
                          "sys 0x%08"NACL_PRIxPTR") "
                          "not in address map\n"),
              nap->break_addr, sys_break);
    }
    ent = NaClVmmapIterStar(&iter);
    NaClLog(4, ("segment containing current break"
                ": page_num 0x%08"NACL_PRIxPTR", npages 0x%"NACL_PRIxS"\n"),
            ent->page_num, ent->npages);
    if (usr_new_last_data_page < ent->page_num + ent->npages) {
      NaClLog(4, "new break within break segment, just bumping addr\n");
      nap->break_addr = new_break;
      break_addr = new_break;
    } else {
      NaClVmmapIterIncr(&iter);
      if (!NaClVmmapIterAtEnd(&iter)
          && ((next_ent = NaClVmmapIterStar(&iter))->page_num
              <= last_internal_page)) {
        /* ran into next segment! */
        NaClLog(4,
                ("new break request of usr address "
                 "0x%08"NACL_PRIxPTR" / usr page 0x%"NACL_PRIxPTR
                 " runs into next region, page_num 0x%"NACL_PRIxPTR", "
                 "npages 0x%"NACL_PRIxS"\n"),
                new_break, usr_new_last_data_page,
                next_ent->page_num, next_ent->npages);
        goto cleanup;
      }
      NaClLog(4,
              "extending segment: page_num 0x%08"NACL_PRIxPTR", "
              "npages 0x%"NACL_PRIxS"\n",
              ent->page_num, ent->npages);
      /* go ahead and extend ent to cover, and make pages accessible */
      start_new_region = (ent->page_num + ent->npages) << NACL_PAGESHIFT;
      ent->npages = (last_internal_page - ent->page_num + 1);
      region_size = (((last_internal_page + 1) << NACL_PAGESHIFT)
                     - start_new_region);

      if (NaClMprotect((void *) NaClUserToSys(nap, start_new_region),
                            region_size,
                            PROT_READ | PROT_WRITE)) {
        NaClLog(LOG_FATAL,
                ("Could not mprotect(0x%08"NACL_PRIxPTR", "
                 "0x%08"NACL_PRIxPTR", "
                 "PROT_READ|PROT_WRITE)\n"),
                start_new_region,
                region_size);
      }
      NaClLog(4, "segment now: page_num 0x%08"NACL_PRIxPTR", "
              "npages 0x%"NACL_PRIxS"\n",
              ent->page_num, ent->npages);
      nap->break_addr = new_break;
      break_addr = new_break;
    }
    /*
     * Zero out memory between old break and new break.
     */
    ASSERT(sys_new_break > sys_break);
    memset((void *) sys_break, 0, sys_new_break - sys_break);
  }

cleanup:
  NaClXMutexUnlock(&nap->mu);
cleanup_no_lock:

  /*
   * This cast is safe because the incoming value (new_break) cannot
   * exceed the user address space--even though its type (uintptr_t)
   * theoretically allows larger values.
   */
  rv = (int32_t) break_addr;
   #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceBrk(nap->cage_id, new_break, rv, totaltime); 
  #endif

  NaClLog(3, "NaClSysBrk: returning 0x%08"NACL_PRIx32"\n", rv);
  return rv;
}

int NaClAclBypassChecks = 0;

void NaClInsecurelyBypassAllAclChecks(void) {
  NaClLog(LOG_WARNING, "BYPASSING ALL ACL CHECKS\n");
  NaClAclBypassChecks = 1;
}

int NaClHighResolutionTimerEnabled(void) {
  return NaClAclBypassChecks;
}

/*
 * NaClOpenAclCheck: Is the NaCl app authorized to open this file?  The
 * return value is syscall return convention, so 0 is success and
 * small negative numbers are negated errno values.
 */
int32_t NaClOpenAclCheck(struct NaClApp *nap,
                         char const     *path,
                         int            flags,
                         int            mode) {
  /*
   * TODO(bsy): provide some minimal authorization check, based on
   * whether a debug flag is set; eventually provide a data-driven
   * authorization configuration mechanism, perhaps persisted via
   * gears.  need GUI for user configuration, as well as designing an
   * appropriate language (with sufficient expressiveness), however.
   */
  NaClLog(1, "NaClOpenAclCheck(0x%08"NACL_PRIxPTR", %s, 0%o, 0%o)\n",
          (uintptr_t) nap, path, flags, mode);
  NaClLog(4, "O_ACCMODE: 0%o\n", flags & NACL_ABI_O_ACCMODE);
  NaClLog(4, "O_RDONLY = %d\n", NACL_ABI_O_RDONLY);
  NaClLog(4, "O_WRONLY = %d\n", NACL_ABI_O_WRONLY);
  NaClLog(4, "O_RDWR   = %d\n", NACL_ABI_O_RDWR);
#define FLOG(VAR, BIT) NaClLog(1, "%s: %s\n", #BIT, ((VAR) & (BIT)) ? "yes" : "no")
    FLOG(flags, NACL_ABI_O_CREAT);
    FLOG(flags, NACL_ABI_O_TRUNC);
    FLOG(flags, NACL_ABI_O_APPEND);
#undef FLOG
  if (NaClAclBypassChecks) {
    return 0;
  }
  return -NACL_ABI_EACCES;
}

/*
 * NaClStatAclCheck: Is the NaCl app authorized to stat this pathname?  The
 * return value is syscall return convention, so 0 is success and
 * small negative numbers are negated errno values.
 *
 * This is primarily for debug use.  File access should be through
 * SRPC-based file servers.
 */
int32_t NaClStatAclCheck(struct NaClApp *nap,
                         char const     *path) {
  NaClLog(2,
          "NaClStatAclCheck(0x%08"NACL_PRIxPTR", %s)\n", (uintptr_t) nap, path);
  if (NaClAclBypassChecks) {
    return 0;
  }
  return -NACL_ABI_EACCES;
}

int32_t NaClIoctlAclCheck(struct NaClApp  *nap, //Unused for now
                          struct NaClDesc *ndp,
                          unsigned long   request,
                          void            *arg) {
  NaClLog(2,
          ("NaClIoctlAclCheck(0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR","
           " %lu, 0x%08"NACL_PRIxPTR"\n"),
          (uintptr_t) nap, (uintptr_t) ndp, request, (uintptr_t) arg);
  if (NaClAclBypassChecks) {
    return 0;
  }
  return -NACL_ABI_EINVAL;
}

int32_t NaClSysGetpid(struct NaClAppThread *natp) {
  int32_t pid;
  struct NaClApp *nap = natp->nap;

    NaClLog(2, "Cage %d Entered NaClSysGetpid(0x%08"NACL_PRIxPTR")\n", nap->cage_id, (uintptr_t)natp);

    #ifdef TRACING
    long long starttime = gettimens();
    #endif

    pid = lind_getpid(nap->cage_id);

    #ifdef TRACING
    long long endtime = gettimens();
    long long totaltime = endtime - starttime;
    NaClStraceGetpid(nap->cage_id, pid, totaltime);
    #endif

    NaClLog(2, "NaClSysGetpid: returning %d\n", pid);

  return pid;
}

int32_t NaClSysGetppid(struct NaClAppThread *natp) {
  int32_t ppid;
  struct NaClApp *nap = natp->nap;

  NaClLog(2, "Cage %d Entered NaClSysGetppid(0x%08"NACL_PRIxPTR")\n", nap->cage_id, (uintptr_t)natp);

  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  ppid = lind_getppid(nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceGetppid(nap->cage_id, ppid, totaltime);
  #endif

  NaClLog(2, "NaClSysGetppid: returning %d\n", ppid);

  return ppid;
}

int32_t NaClSysExit(struct NaClAppThread *natp, int status) {
    struct NaClApp *nap = natp->nap;

    // First, signal any other threads in the group to exit
    if (nap->num_threads > 1) {
        NaClExitThreadGroup(natp);
    }

    #ifdef TRACING
    long long starttime = gettimens();
    #endif

    NaClLog(1, "Exit syscall handler: %d\n", status);
    (void)NaClReportExitStatus(nap, NACL_ABI_W_EXITCODE(status, 0)); // need to report here first so we add exited process as a zombie for wait
    lind_exit(status, nap->cage_id); // before lind_exit sends SIGCHLD

    #ifdef TRACING
    long long endtime = gettimens();
    long long totaltime = endtime - starttime;
    NaClStraceExit(nap->cage_id, status, totaltime);
    #endif

    NaClAppThreadTeardown(natp);

    /* NOTREACHED */
    return -NACL_ABI_EINVAL;
}

int32_t NaClSysThreadExit(struct NaClAppThread *natp, int32_t *stack_flag) {
    uint32_t zero = 0;

    
    NaClLog(4, "NaClSysThreadExit(0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR"\n",
            (uintptr_t)natp,
            (uintptr_t)stack_flag);

    /*
     * NB: NaClThreads are never joinable, but the abstraction for NaClApps
     * are.
     */

    if (stack_flag) {
        NaClLog(2, "NaClSysThreadExit: stack_flag is %"NACL_PRIxPTR"\n", (uintptr_t)stack_flag);
        if (!NaClCopyOutToUser(natp->nap, (uintptr_t)stack_flag, &zero, sizeof(zero))) {
            NaClLog(2, "NaClSysThreadExit: ignoring invalid"
                    " stack_flag 0x%"NACL_PRIxPTR"\n",
                    (uintptr_t)stack_flag);
        }
    }

  struct NaClThread *host_thread;
  host_thread = &natp->host_thread;
  lindthreadremove(natp->nap->cage_id, host_thread->tid); // remove from rustposix kill map

    
    NaClAppThreadTeardown(natp);

  /* NOTREACHED */
  return -NACL_ABI_EINVAL;
}

int32_t NaClSysNameService(struct NaClAppThread *natp, int32_t *desc_addr) {
    struct NaClApp *nap = natp->nap;
    int32_t retval = -NACL_ABI_EINVAL;
    int32_t desc;


    NaClLog(3,
            ("NaClSysNameService(0x%08"NACL_PRIxPTR","
            " 0x%08"NACL_PRIxPTR")\n"),
            (uintptr_t)natp,
            (uintptr_t)desc_addr);

    if (!NaClCopyInFromUser(nap, &desc, (uintptr_t)desc_addr, sizeof(desc))) {
        NaClLog(LOG_ERROR,
                "Invalid address argument to NaClSysNameService\n");
        retval = -NACL_ABI_EFAULT;
        goto done;
    }

    if (-1 == desc) {
        /* read */
        desc = NaClSetAvail(nap, NaClDescRef(nap->name_service_conn_cap));
        if (NaClCopyOutToUser(nap, (uintptr_t)desc_addr,
                            &desc, sizeof(desc))) {
            retval = 0;
        } else {
            retval = -NACL_ABI_EFAULT;
        }
    } else {
        struct NaClDesc *desc_obj_ptr = NaClGetDesc(nap, desc);

        if (!desc_obj_ptr) {
            retval = -NACL_ABI_EBADF;
            goto done;
        }
        if (NACL_DESC_CONN_CAP != NACL_VTBL(NaClDesc, desc_obj_ptr)->typeTag &&
            NACL_DESC_CONN_CAP_FD != NACL_VTBL(NaClDesc, desc_obj_ptr)->typeTag) {
            retval = -NACL_ABI_EINVAL;
            goto done;
        }
        /* write */
        NaClXMutexLock(&nap->mu);
        NaClDescUnref(nap->name_service_conn_cap);
        nap->name_service_conn_cap = desc_obj_ptr;
        NaClXMutexUnlock(&nap->mu);
        retval = 0;
    }

done:
  return retval;
}

int32_t NaClSysDup(struct NaClAppThread *natp, int oldfd) {
  struct NaClApp *nap = natp->nap;
  int ret;

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  NaClLog(1, "NaClSysDup(0x%08"NACL_PRIxPTR", %d)\n", (uintptr_t)natp, oldfd);

  if (oldfd < 0) return -NACL_ABI_EBADF;

  ret = lind_dup(oldfd, nap->cage_id);
   #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceDup(nap->cage_id, oldfd, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysDup2(struct NaClAppThread  *natp,
                    int                   oldfd,
                    int                   newfd) {
  struct NaClApp *nap = natp->nap;
  int ret;

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  NaClLog(1, "%s\n", "[dup2] Entered dup2!");
  NaClLog(1, "[dup2] cage id = %d \n", nap->cage_id);
  NaClLog(1, "[dup2] oldfd = %d \n", oldfd);
  NaClLog(1, "[dup2] newfd = %d \n", newfd);

  if ((newfd < 0) || (oldfd < 0)) return -NACL_ABI_EBADF;

  ret = lind_dup2(oldfd, newfd, nap->cage_id);


  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceDup2(nap->cage_id, oldfd, newfd, ret,totaltime);
  #endif

  return ret;
}

int32_t NaClSysDup3(struct NaClAppThread  *natp,
                    int                   oldfd,
                    int                   newfd,
                    int                   flags) {
  struct NaClApp *nap = natp->nap;

  NaClLog(1, "%s\n", "[dup3] Entered dup3!");
  NaClLog(1, "[dup3] cage id = %d \n", nap->cage_id);
  NaClLog(1, "[dup3] oldfd = %d \n", oldfd);
  NaClLog(1, "[dup3] newfd = %d \n", newfd);

  UNREFERENCED_PARAMETER(nap);
  UNREFERENCED_PARAMETER(flags);

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  int ret = NaClSysDup2(natp, oldfd, newfd);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceDup3(nap->cage_id, oldfd, newfd, flags,ret, totaltime);
  #endif

  return ret;
}

static uint32_t CopyPathFromUser(struct NaClApp *nap,
                                 char           *dest,
                                 size_t         num_bytes,
                                 uintptr_t      src) {
  /*
   * NaClCopyInFromUserZStr may (try to) get bytes that is outside the
   * app's address space and generate a fault.
   */
  if (!NaClCopyInFromUserZStr(nap, dest, num_bytes, src)) {
    if (dest[0] == '\0') {
      NaClLog(LOG_ERROR, "NaClSys: invalid address for pathname\n");
      return -NACL_ABI_EFAULT;
    }

    NaClLog(LOG_ERROR, "NaClSys: pathname string too long\n");
    return -NACL_ABI_ENAMETOOLONG;
  }

  return 0;
}

int32_t NaClSysOpen(struct NaClAppThread  *natp,
                    char                  *pathname,
                    int                   flags,
                    int                   mode) {
  struct NaClApp       *nap = natp->nap;
  int                  retval = -NACL_ABI_EINVAL;
  char                 path[NACL_CONFIG_PATH_MAX];
  int                  allowed_flags;
  const char           *glibc_prefix = "/lib/glibc/";
  const char           *tls_prefix = "/lib/glibc/tls/";
  const size_t         tls_start_idx = strlen(glibc_prefix);
  const size_t         tls_end_idx = strlen(tls_prefix);
  int                  userfd;


  NaClLog(2, "NaClSysOpen(0x%08"NACL_PRIxPTR", "
          "0x%08"NACL_PRIxPTR", 0x%x, 0x%x)\n",
          (uintptr_t)natp, (uintptr_t)pathname, flags, mode);

  retval = CopyPathFromUser(nap, path, sizeof(path), (uintptr_t)pathname);

  /*
   * TODO:
   * find a cleaner method to prevent the
   * runtime linker from searching for libc
   * in /lib/glibc/tls/ (this removes the
   * tls/ path component).
   *
   * -jp
   */
  if (!memcmp(path, tls_prefix, sizeof(tls_prefix) - 1)) {
    char *left_side = path + tls_start_idx;
    char *right_side = path + tls_end_idx;
    size_t len_of_rest = strlen(right_side) + 1;
    memmove(left_side, right_side, len_of_rest);
  }

  if (retval) return retval;
  

  allowed_flags = (NACL_ABI_O_ACCMODE | NACL_ABI_O_CREAT
                   | NACL_ABI_O_TRUNC | NACL_ABI_O_APPEND | 
                   NACL_ABI_O_CLOEXEC);
  if (flags & ~allowed_flags) {
    NaClLog(1, "Invalid open flags 0%o, ignoring extraneous bits\n", flags);
    flags &= allowed_flags;
  }
  if (mode & ~0600) {
    NaClLog(1, "IGNORING Invalid access mode bits 0%o\n", mode);
    mode &= 0600;
  }
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = NaClOpenAclCheck(nap, path, flags, mode);
  if (retval) {
    NaClLog(2, "Open ACL check rejected \"%s\".\n", path);
    return retval;
  }
  userfd = lind_open(path, flags, mode, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceOpen(nap->cage_id, path, flags, mode, userfd, totaltime);
  #endif

  return userfd;
}

int32_t NaClSysClose(struct NaClAppThread *natp, int d) {
  struct NaClApp *nap = natp->nap;

  NaClLog(1, "Cage %d Entered NaClSysClose(0x%08"NACL_PRIxPTR", %d)\n",
            nap->cage_id, (uintptr_t) natp, d);
  int ret = -NACL_ABI_EBADF;

  if (d < 0) return -NACL_ABI_EBADF;

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_close(d, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceClose(nap->cage_id, d, ret,totaltime);
  #endif

  return ret;
}

int32_t NaClSysGetdents(struct NaClAppThread *natp,
                        int                  d,
                        void                 *dirp,
                        size_t               count) {
    struct NaClApp *nap = natp->nap;

    NaClLog(1, "Entered NaClSysGetdents(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR", %"NACL_PRIdS"[0x%"NACL_PRIxS"])\n",
            (uintptr_t) natp, d, (uintptr_t) dirp, count, count);

    int32_t retval = -NACL_ABI_EINVAL;
    ssize_t getdents_ret;
    uintptr_t sysaddr;


    /*
     * Generic NaClCopyOutToUser is not sufficient, since buffer size
     * |count| is arbitrary and we wouldn't want to have to allocate
     * memory in trusted address space to match.
     */
    sysaddr = NaClUserToSysAddrRangeProt(nap, (uintptr_t) dirp, count, NACL_ABI_PROT_WRITE);
    if (kNaClBadAddress == sysaddr) {
        NaClLog(4, " illegal address for directory data\n");
        return -NACL_ABI_EFAULT;
    }

    /*
     * Clamp count to INT32_MAX to avoid the possibility of Getdents returning
     * a value that is outside the range of an int32.
     */
    if (count > INT32_MAX) {
        count = INT32_MAX;
    }

    /*
     * Grab addr space lock; getdents should not normally block, though
     * if the directory is on a networked filesystem this could, and
     * cause mmap to be slower on Windows.
     */
    NaClXMutexLock(&nap->mu);
    #ifdef TRACING
    long long starttime = gettimens();
    #endif
    getdents_ret = lind_getdents(d, (void *)sysaddr, count, nap->cage_id);
    NaClXMutexUnlock(&nap->mu);  // Drop addr space lock

    if ((getdents_ret < INT32_MIN && !NaClSSizeIsNegErrno(&getdents_ret)) || INT32_MAX < getdents_ret) {
        // This should never happen, because we already clamped the input count
        NaClLog(LOG_FATAL, "Overflow in Getdents: return value is %"NACL_PRIxS, getdents_ret);
    } else {
        retval = (int32_t)getdents_ret;
    }

    if (retval > 0) {
        NaClLog(4, "getdents returned %d bytes\n", retval);
        NaClLog(8, "getdents result: %.*s\n", retval, (char *)sysaddr);
    } else {
        NaClLog(4, "getdents returned %d\n", retval);
    }

    #ifdef TRACING
    long long endtime = gettimens();
    long long totaltime = endtime - starttime;
    NaClStraceGetdents(nap->cage_id, d, dirp, count, retval, totaltime); // Corrected arguments
    #endif

    return retval;
}

// Lind: I/O calls.
// We've removed the VMHole functions because they are specific to the windows VM concern (see note in sel_ldr.h)
int32_t NaClSysRead(struct NaClAppThread  *natp,
                    int                   d,
                    void                  *buf,
                    size_t                count) {
  struct NaClApp  *nap = natp->nap;
  int32_t         retval = -NACL_ABI_EINVAL;
  ssize_t         read_result = -NACL_ABI_EINVAL;
  uintptr_t       sysaddr;

  #ifdef TRACING
  // Get the start time
  long long starttime = gettimens();
  #endif
  
  // NaClLog(2, "Cage %d Entered NaClSysRead(0x%08"NACL_PRIxPTR", "
  //          "%d, 0x%08"NACL_PRIxPTR", "
  //          "%"NACL_PRIdS"[0x%"NACL_PRIxS"])\n",
  //         nap->cage_id, (uintptr_t) natp, d, (uintptr_t) buf, count, count);
  if (d < 0) return -NACL_ABI_EBADF;

  sysaddr = NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, count, NACL_ABI_PROT_WRITE);

  if (kNaClBadAddress == sysaddr) return -NACL_ABI_EFAULT;

  /*
   * The maximum length for read and write is INT32_MAX--anything larger and
   * the return value would overflow. Passing larger values isn't an error--
   * we'll just clamp the request size if it's too large.
   */
  if (count > INT32_MAX) {
    count = INT32_MAX;
  }

  /* Lind - we removed the VMIOWillStart and End functions here, which is fine for Linux
   * See note in sel_ldr.h
   *
   * We bypass the dispatcher with quick_read to patch directly to RustPOSIX for performance efficiency 
   */
  
  read_result = quick_read(d, (void *)sysaddr, count, nap->cage_id);

/* This cast is safe because we clamped count above.*/
  retval = (int32_t) read_result;

  #ifdef TRACING
  // Get the end time
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  
  NaClStraceRead(nap->cage_id, d, (void *)sysaddr, count, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysPread(struct NaClAppThread *natp,
                     int d,
                     void *buf,
                     size_t count,
                     off_t offset) { 
  struct NaClApp *nap = natp->nap;
  int32_t retval = -NACL_ABI_EINVAL;
  ssize_t read_result = -NACL_ABI_EINVAL;
  uintptr_t sysaddr;
  size_t log_bytes;
  char const *ellipsis = "";



  NaClLog(2, "Cage %d Entered NaClSysPRead(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR", %"NACL_PRIdS"[0x%"NACL_PRIxS"])\n",nap->cage_id, (uintptr_t) natp, d, (uintptr_t) buf, count, count);

    if (d < 0) return -NACL_ABI_EBADF;

  sysaddr = NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, count, NACL_ABI_PROT_READ);
  if (kNaClBadAddress == sysaddr) return -NACL_ABI_EFAULT;

  /*
    * The maximum length for read and write is INT32_MAX--anything larger and
    * the return value would overflow. Passing larger values isn't an error--
    * we'll just clamp the request size if it's too large.
    */
  if (count > INT32_MAX) {
      count = INT32_MAX;
  }
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  read_result = lind_pread(d, (void *)sysaddr, count, offset, nap->cage_id);

  if (read_result > 0) {
      NaClLog(4, "pread returned %"NACL_PRIdS" bytes\n", read_result);
      log_bytes = (size_t) read_result;
      if (log_bytes > INT32_MAX) {
          log_bytes = INT32_MAX;
          ellipsis = "...";
      }
      if (log_bytes > kdefault_io_buffer_bytes_to_log) {
          log_bytes = kdefault_io_buffer_bytes_to_log;
          ellipsis = "...";
      }
      NaClLog(8, "pread result: %.*s%s\n",
              (int) log_bytes, (char *) sysaddr, ellipsis);
  } else {
      NaClLog(4, "pread returned %"NACL_PRIdS"\n", read_result);
  }

/* This cast is safe because we clamped count above.*/
  retval = (int32_t) read_result;

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStracePread(nap->cage_id, d, (void *)sysaddr, count, offset, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysWrite(struct NaClAppThread *natp,
                     int d,
                     void *buf,
                     size_t count) {
  struct NaClApp *nap = natp->nap;
  int32_t retval = -NACL_ABI_EINVAL;
  ssize_t write_result = -NACL_ABI_EINVAL;
  uintptr_t sysaddr;

  NaClLog(2, "Cage %d Entered NaClSysWrite(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR", %"NACL_PRIdS"[0x%"NACL_PRIxS"])\n",
          nap->cage_id, (uintptr_t) natp, d, (uintptr_t) buf, count, count);

  if (d < 0) return -NACL_ABI_EBADF;

  sysaddr = NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, count, NACL_ABI_PROT_READ);
  if (kNaClBadAddress == sysaddr) return -NACL_ABI_EFAULT;

    /*
     * The maximum length for read and write is INT32_MAX--anything larger and
     * the return value would overflow. Passing larger values isn't an error--
     * we'll just clamp the request size if it's too large.
     */
  count = count > INT32_MAX ? INT32_MAX : count;

    /* Lind - we removed the VMIOWillStart and End functions here, which is fine for Linux
     * See note in sel_ldr.h
     *
     * We bypass the dispatcher with quick_write to patch directly to RustPOSIX for performance efficiency 
     */
  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  write_result = quick_write(d, (void *)sysaddr, count, nap->cage_id);

/* This cast is safe because we clamped count above.*/
  retval = (int32_t)write_result;

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceWrite(nap->cage_id, d, (void *) sysaddr, count, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysPwrite(struct NaClAppThread *natp,
                      int d,
                      const void *buf,
                      size_t count,
                      off_t offset) {
  struct NaClApp *nap = natp->nap;
  int32_t retval = -NACL_ABI_EINVAL;
  ssize_t write_result = -NACL_ABI_EINVAL;
  uintptr_t sysaddr;
  char const *ellipsis = "";
  size_t log_bytes;

   

    NaClLog(2, "Cage %d Entered NaClSysPWrite(0x%08"NACL_PRIxPTR", "
            "%d, 0x%08"NACL_PRIxPTR", "
            "%"NACL_PRIdS"[0x%"NACL_PRIxS"])\n",
            nap->cage_id, (uintptr_t) natp, d, (uintptr_t) buf, count, count);

    if (d < 0) return -NACL_ABI_EBADF;

    sysaddr = NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, count, NACL_ABI_PROT_READ);
    if (kNaClBadAddress == sysaddr) return retval = -NACL_ABI_EFAULT;

    /*
     * The maximum length for read and write is INT32_MAX--anything larger and
     * the return value would overflow. Passing larger values isn't an error--
     * we'll just clamp the request size if it's too large.
     */
    count = count > INT32_MAX ? INT32_MAX : count;
    log_bytes = count;
    if (log_bytes == INT32_MAX) {
        ellipsis = "...";
    }
    UNREFERENCED_PARAMETER(ellipsis);
    if (log_bytes > kdefault_io_buffer_bytes_to_log) {
        log_bytes = kdefault_io_buffer_bytes_to_log;
        ellipsis = "...";
    }
    UNREFERENCED_PARAMETER(log_bytes);
    UNREFERENCED_PARAMETER(ellipsis);
    NaClLog(2, "In NaClSysPWrite(%d, %.*s%s, %"NACL_PRIdS")\n",
            d, (int) log_bytes, (char *) sysaddr, ellipsis, count);

    #ifdef TRACING
    long long starttime = gettimens();
    #endif
    write_result = lind_pwrite(d, (void *) sysaddr, count, offset, nap->cage_id);

/* This cast is safe because we clamped count above. */
  retval = (int32_t) write_result;

    #ifdef TRACING
    long long endtime = gettimens();
    long long totaltime = endtime - starttime;
    NaClStracePWrite(nap->cage_id, d, (void *) sysaddr, count, offset, (int) retval, totaltime);
    #endif

  return retval;
}

int32_t NaClSysWritev(struct NaClAppThread *natp,
                     int fd,
                     void *iovec,
                     int iovcnt) {
  struct NaClApp *nap = natp->nap;
  int32_t retval = -NACL_ABI_EINVAL;
  ssize_t write_result = -NACL_ABI_EINVAL;

  NaClLog(2, "Cage %d Entered NaClSysWritev(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR", %"NACL_PRIdS"[0x%"NACL_PRIxS"])\n", nap->cage_id, (uintptr_t) natp, fd, (uintptr_t) iovec, iovcnt);

  if (fd < 0) return -NACL_ABI_EBADF;

  // Convert 32-bit iovec to custom struct for translation

  struct nacl_abi_iovec* useriovec = (struct nacl_abi_iovec*)NaClUserToSysAddrRangeProt(nap, (uintptr_t) iovec, iovcnt, NACL_ABI_PROT_READ);

  struct iovec* sysiovec = (struct iovec*)malloc(iovcnt * sizeof(struct iovec));

  // loop throught custom user iovecs and translate inner iov_base ptrs to newly allocated sys iovec
  for (int i = 0; i < iovcnt; i++) {
    sysiovec[i].iov_base = (void *)NaClUserToSysAddrRangeProt(nap, (uintptr_t) useriovec[i].iov_base, useriovec[i].iov_len, NACL_ABI_PROT_READ);
    if (kNaClBadAddress == sysiovec[i].iov_base) return -NACL_ABI_EFAULT;

  /*
    * The maximum length for read and write is INT32_MAX--anything larger and
    * the return value would overflow. Passing larger values isn't an error--
    * we'll just clamp the request size if it's too large.
    */

    int clamped_iovlen = useriovec[i].iov_len > INT32_MAX ? INT32_MAX : useriovec[i].iov_len;
    sysiovec[i].iov_len = clamped_iovlen;
  }


  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  write_result = lind_writev(fd, sysiovec, iovcnt, nap->cage_id);

/* This cast is safe because we clamped count above.*/
  retval = (int32_t)write_result;

  free(sysiovec);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceWritev(nap->cage_id, fd, (void *) sysiovec, iovcnt, retval, totaltime);
  #endif

  return retval;
}

/*
 * This implements 64-bit offsets, so we use |offp| as an in/out
 * address so we can have a 64 bit return value.
 */
int32_t NaClSysLseek(struct NaClAppThread *natp,
                     int d,
                     nacl_abi_off_t *offp,
                     int whence) {
  struct NaClApp *nap = natp->nap;
  nacl_abi_off_t offset;
  nacl_off64_t retval64;
  int32_t retval = -NACL_ABI_EINVAL;

    #ifdef TRACING
    long long starttime = gettimens();
    #endif

  NaClLog(2, "Entered NaClSysLseek(0x%08"NACL_PRIxPTR", %d,"
          " 0x%08"NACL_PRIxPTR", %d)\n",
          (uintptr_t) natp, d, (uintptr_t) offp, whence);

  if (d < 0) return -NACL_ABI_EBADF;

  if (!NaClCopyInFromUser(nap, &offset, (uintptr_t) offp, sizeof(offset))) return -NACL_ABI_EFAULT;

  NaClLog(4, "offset 0x%08"NACL_PRIxNACL_OFF"\n", offset);

  retval64 = lind_lseek(d, offset, whence, nap->cage_id);

    if (NaClOff64IsNegErrno(&retval64)) {
        retval = (int32_t) retval64;
    } else {
        if (NaClCopyOutToUser(nap, (uintptr_t) offp, &retval64, sizeof(retval64))) {
            retval = 0;
        } else {
            NaClLog(LOG_FATAL,
                    "NaClSysLseek: in/out ptr became invalid at copyout?\n");
        }
    }

    #ifdef TRACING
    long long endtime = gettimens();
    long long totaltime = endtime - starttime;
    NaClStraceLseek(nap->cage_id, d, (uintptr_t) &offset, whence, (int) retval, totaltime);
    #endif

  return retval;
}

int32_t NaClSysIoctl(struct NaClAppThread *natp,
                     int                  d,
                     unsigned long        request,
                     void                 *arg_ptr) {
  struct NaClApp  *nap = natp->nap;
  int             retval = -NACL_ABI_EINVAL;
  uintptr_t       sysaddr;

  NaClLog(2, "Cage %d Entered NaClSysIoctl(0x%08"NACL_PRIxPTR
           ", %d, %lu, 0x%08"NACL_PRIxPTR")\n",
           nap->cage_id, (uintptr_t)natp, d, request,
           (uintptr_t)arg_ptr);
  
  /* NR - currently we only handle FIONBIO and FIOASYNC where arg_ptr is just an int*   */
  sysaddr = NaClUserToSysAddrProt(nap, (uintptr_t) arg_ptr, NACL_ABI_PROT_READ);
  if (kNaClBadAddress == sysaddr) {
    NaClLog(2, "NaClSysIoctl could not translate buffer address, returning%d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  // Further checks might be necessary for ioctl calls with structs or arrays
  // Those calls are not implemented for now
  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  retval = lind_ioctl(d ,request, (void *) sysaddr, nap->cage_id);
  
  NaClLog(2, "NaClSysIoctl: returning %d\n", retval);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceIoctl(nap->cage_id, d, request, (void *) sysaddr, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysFstat(struct NaClAppThread *natp,
                     int                  d,
                     struct nacl_abi_stat *nasp) {
  struct NaClApp        *nap = natp->nap;
  int32_t               retval = -NACL_ABI_EINVAL;
  struct lind_stat  result;

  NaClLog(2, "Entered NaClSysFstat(0x%08"NACL_PRIxPTR
           ", %d, 0x%08"NACL_PRIxPTR")\n",
           (uintptr_t)natp,
           d, (uintptr_t)nasp);

  NaClLog(2, "sizeof(struct nacl_abi_stat) = %"NACL_PRIdS" (0x%"NACL_PRIxS")\n",
          sizeof(*nasp), sizeof(*nasp));

  if (d < 0) return -NACL_ABI_EBADF;
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_fxstat(d, &result, nap->cage_id);
  if (!retval) {
    if (!NaClCopyOutToUser(nap, (uintptr_t) nasp, &result, sizeof(result))) return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceFstat(nap->cage_id, d, (uintptr_t) &result, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysStat(struct NaClAppThread  *natp,
                    const char            *pathname,
                    struct nacl_abi_stat  *buf) {
    struct NaClApp      *nap = natp->nap;
    int32_t             retval;
    char                path[NACL_CONFIG_PATH_MAX];
    struct lind_stat    result;
    long long           starttime = 0;
    long long           endtime = 0;
    long long           totaltime = 0;

    NaClLog(2, "Entered NaClSysStat(0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR","
           " 0x%08"NACL_PRIxPTR")\n",
          (uintptr_t)natp, (uintptr_t)pathname,(uintptr_t)buf);

  retval = CopyPathFromUser(nap, path,sizeof(path),(uintptr_t) pathname);
  if (retval) return -NACL_ABI_EINVAL;

    retval = NaClStatAclCheck(nap, path);
    if (retval) return -NACL_ABI_EINVAL;
    #ifdef TRACING
    starttime = gettimens();
    #endif

  retval = lind_xstat(path, &result, nap->cage_id);
  if (!retval) {
    if (!NaClCopyOutToUser(nap, (uintptr_t) buf, &result, sizeof(result))) return -NACL_ABI_EFAULT;
  }

    #ifdef TRACING
    endtime = gettimens();
    totaltime = endtime - starttime;
    NaClStraceStat(nap->cage_id, path, &result, retval, totaltime);
    #endif

  return retval;
}

int32_t NaClSysLStat(struct NaClAppThread  *natp,
                    const char            *pathname,
                    struct nacl_abi_stat  *buf) {
  struct NaClApp      *nap = natp->nap;
  int32_t             retval;
  char                path[NACL_CONFIG_PATH_MAX];
  struct lind_stat  result;

  NaClLog(2, "Entered NaClSysLStat(0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR","
           " 0x%08"NACL_PRIxPTR")\n",
          (uintptr_t)natp,(uintptr_t)pathname, (uintptr_t)buf);

  retval = CopyPathFromUser(nap, path, sizeof(path), (uintptr_t) pathname);
  if (retval) return -NACL_ABI_EINVAL;

  retval = NaClStatAclCheck(nap, path);
  if (retval) return -NACL_ABI_EINVAL;
  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  retval = lind_xstat(path, &result, nap->cage_id);
  if (!retval) {
    if (!NaClCopyOutToUser(nap, (uintptr_t) buf, &result, sizeof(result))) return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceLStat(nap->cage_id, path, &result, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysMkdir(struct NaClAppThread *natp,
                     uint32_t             pathname,
                     int                  mode) {
  struct NaClApp *nap = natp->nap;
  char           path[NACL_CONFIG_PATH_MAX];
  int32_t        retval = -NACL_ABI_EINVAL;

  NaClLog(2, "Cage %d Entered NaClSysMkdir(0x%08"NACL_PRIxPTR", "
          "%d, %d)\n", nap->cage_id, (uintptr_t) natp, pathname, mode);

  if (!NaClAclBypassChecks) return -NACL_ABI_EACCES;

  retval = CopyPathFromUser(nap, path, sizeof(path), pathname);
  if (retval) return;

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_mkdir(path, mode, natp->nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceMkdir(nap->cage_id, path, mode, retval, totaltime);
  #endif

  NaClLog(2, "NaClSysMkdir: returning %d\n", retval);

  return retval;
}

int32_t NaClSysRmdir(struct NaClAppThread *natp,
                     uint32_t             pathname) {
    struct NaClApp *nap = natp->nap;
    char path[NACL_CONFIG_PATH_MAX];
    int32_t retval;
    long long starttime = 0;
    long long endtime = 0;
    long long totaltime = 0;

  NaClLog(2, "Cage %d Entered NaClSysRmdir(0x%08"NACL_PRIxPTR", "
          "%d)\n", nap->cage_id, (uintptr_t) natp, pathname);

  if (!NaClAclBypassChecks) return -NACL_ABI_EACCES;

  retval = CopyPathFromUser(nap, path, sizeof(path), pathname);
  if (retval) return -NACL_ABI_EINVAL;

    #ifdef TRACING
    starttime = gettimens();
    #endif

  retval = lind_rmdir(path, natp->nap->cage_id);

    #ifdef TRACING
    endtime = gettimens();
    totaltime = endtime - starttime;
    NaClStraceRmdir(nap->cage_id, path, retval, totaltime);
    #endif

    NaClLog(2, "NaClSysRmdir: returning %d\n", retval);

  return retval;
}

int32_t NaClSysChdir(struct NaClAppThread *natp,
                     uint32_t             pathname) {
  struct NaClApp *nap = natp->nap;
  char path[NACL_CONFIG_PATH_MAX];
  int32_t retval;
  long long starttime = 0;
  long long endtime = 0;
  long long totaltime = 0;

  NaClLog(2, "Cage %d Entered NaClSysChdir(0x%08"NACL_PRIxPTR", "
          "%d)\n", nap->cage_id, (uintptr_t) natp, pathname);

  if (!NaClAclBypassChecks) return -NACL_ABI_EACCES;

  retval = CopyPathFromUser(nap, path, sizeof(path), pathname);
  if (retval) return -NACL_ABI_EINVAL;

  #ifdef TRACING
  starttime = gettimens();
  #endif

  retval = lind_chdir(path, natp->nap->cage_id);

  #ifdef TRACING
  endtime = gettimens();
  totaltime = endtime - starttime;
  NaClStraceChdir(nap->cage_id, path, retval, totaltime);
  #endif

  NaClLog(2, "NaClSysChdir: returning %d\n", retval);

  return retval;
}

int32_t NaClSysChmod(struct NaClAppThread *natp,
                     uint32_t             pathname,
                     int                  mode) {
    struct NaClApp *nap = natp->nap;
    char path[NACL_CONFIG_PATH_MAX];
    int32_t retval;
    long long starttime = 0;
    long long endtime = 0;
    long long totaltime = 0;

  NaClLog(2, "Cage %d Entered NaClSysChmod(0x%08"NACL_PRIxPTR", "
          "%d, %d)\n", nap->cage_id, (uintptr_t) natp, pathname, mode);

  if (!NaClAclBypassChecks) return -NACL_ABI_EACCES;

  retval = CopyPathFromUser(nap, path, sizeof(path), pathname);
  if (retval) return -NACL_ABI_EINVAL;

  #ifdef TRACING
  starttime = gettimens();
  #endif

  retval = lind_chmod(path, mode, natp->nap->cage_id);

  #ifdef TRACING
  endtime = gettimens();
  totaltime = endtime - starttime;
  NaClStraceChmod(nap->cage_id, path, mode, retval, totaltime);
  #endif

  NaClLog(2, "NaClSysChmod: returning %d\n", retval);

  return retval;
}

int32_t NaClSysFchmod(struct NaClAppThread *natp,
                    int                   fd,
                    int                  mode) {
  struct NaClApp *nap = natp->nap;
  int32_t retval;
  long long starttime = 0;
  long long endtime = 0;
  long long totaltime = 0;

  NaClLog(2, "Cage %d Entered NaClSysFchmod(0x%08"NACL_PRIxPTR", "
           "%d, %d)\n", nap->cage_id, (uintptr_t) natp, fd, mode);

  if (!NaClAclBypassChecks) return -NACL_ABI_EACCES;

  #ifdef TRACING
  starttime = gettimens();
  #endif

  retval = lind_fchmod(fd, mode, nap->cage_id);

  #ifdef TRACING
  endtime = gettimens();
  totaltime = endtime - starttime;
  NaClStraceFchmod(nap->cage_id, fd, mode, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysFchdir(struct NaClAppThread *natp, int fd) {
    int32_t ret;
    struct NaClApp *nap = natp->nap;
    long long starttime = 0;
    long long endtime = 0;
    long long totaltime = 0;

  NaClLog(2, "Cage %d Entered NaClSysFchdir(0x%08"NACL_PRIxPTR", %d)\n",
          nap->cage_id, (uintptr_t) natp, fd);

    #ifdef TRACING
    starttime = gettimens();
    #endif

  ret = lind_fchdir(fd, nap->cage_id);

    #ifdef TRACING
    endtime = gettimens();
    totaltime = endtime - starttime;
    NaClStraceFchdir(nap->cage_id, fd, ret, totaltime);
    #endif

  return ret;
}

int32_t NaClSysFsync(struct NaClAppThread *natp, int fd) {
    int32_t ret;
    struct NaClApp *nap = natp->nap;
    long long starttime = 0;
    long long endtime = 0;
    long long totaltime = 0;

  NaClLog(2, "Cage %d Entered NaClSysFsync(0x%08"NACL_PRIxPTR", %d)\n",
            nap->cage_id, (uintptr_t) natp, fd);

  #ifdef TRACING
  starttime = gettimens();
  #endif

  ret = lind_fsync(fd, nap->cage_id);

    #ifdef TRACING
    endtime = gettimens();
    totaltime = endtime - starttime;
    NaClStraceFsync(nap->cage_id, fd, ret, totaltime);
    #endif

  return ret;
}

int32_t NaClSysFdatasync(struct NaClAppThread *natp, int fd) {
    int32_t ret;
    struct NaClApp *nap = natp->nap;

    #ifdef TRACING
    long long starttime = gettimens();
    #endif

  NaClLog(2, "Cage %d Entered NaClSysFdatasync(0x%08"NACL_PRIxPTR", %d)\n",
          nap->cage_id, (uintptr_t) natp, fd);

  ret = lind_fdatasync(fd, nap->cage_id);

    #ifdef TRACING
    long long endtime = gettimens();
    long long totaltime = endtime - starttime;
    NaClStraceFdatasync(nap->cage_id, fd, ret, totaltime);
    #endif

  return ret;
}

int32_t NaClSysSyncFileRange(struct NaClAppThread *natp,int fd,off_t offset,off_t nbytes,uint32_t flags) {
    int32_t ret;
    struct NaClApp *nap = natp->nap;
    long long starttime = 0;
    long long endtime = 0;
    long long totaltime = 0;

  NaClLog(2, "Cage %d Entered NaClSysSyncFileRange(0x%08"NACL_PRIxPTR", %d)\n",
          nap->cage_id, (uintptr_t) natp, fd);

    #ifdef TRACING
    starttime = gettimens();
    #endif

  ret = lind_sync_file_range(fd, offset, nbytes, flags, nap->cage_id);

    #ifdef TRACING
    endtime = gettimens();
    totaltime = endtime - starttime;
    NaClStraceSyncFileRange(nap->cage_id, fd, offset, nbytes, flags, ret, totaltime);
    #endif

  return ret;
}

int32_t NaClSysGetcwd(struct NaClAppThread *natp, char *buf, size_t size) {
    struct NaClApp *nap = natp->nap;
    uintptr_t sysaddr;
    int32_t retval;

    NaClLog(2, "Cage %d Entered NaClSysGetcwd(0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR", %lx)\n",
            nap->cage_id, (uintptr_t)natp, (uintptr_t)buf, size);

  if (!NaClAclBypassChecks) return -NACL_ABI_EACCES;

    if (size >= NACL_CONFIG_PATH_MAX) {
        size = NACL_CONFIG_PATH_MAX - 1;
    }

    sysaddr = NaClUserToSysAddrRangeProt(nap, (uintptr_t)buf, size, NACL_ABI_PROT_READ);
    if (kNaClBadAddress == sysaddr) {
        NaClLog(2, "NaClSysGetcwd could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
        return -NACL_ABI_EFAULT;
    }

    #ifdef TRACING
    long long starttime = gettimens();
    #endif

  retval = lind_getcwd((void *)sysaddr, size, natp->nap->cage_id);

    #ifdef TRACING
    long long endtime = gettimens();
    long long totaltime = endtime - starttime;
    NaClStraceGetcwd(nap->cage_id, sysaddr, size, retval, totaltime);
    #endif

    NaClLog(2, "NaClSysGetcwd: returning %d\n", retval);

  return retval;
}

int32_t NaClSysLink(struct NaClAppThread *natp, char* from, char* to) {
  struct NaClApp *nap = natp->nap;
  char srcpath[NACL_CONFIG_PATH_MAX];
  char dstpath[NACL_CONFIG_PATH_MAX];
  int32_t retval;

  if ((retval = CopyPathFromUser(nap, srcpath, sizeof(srcpath), (uintptr_t) from))) return -NACL_ABI_EINVAL;
  
  if ((retval = CopyPathFromUser(nap, dstpath, sizeof(dstpath), (uintptr_t) to))) return -NACL_ABI_EINVAL;

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_link(srcpath, dstpath, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceLink(nap->cage_id, srcpath, dstpath, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysUnlink(struct NaClAppThread *natp, char* pathname) {
  struct NaClApp *nap = natp->nap;
  char path[NACL_CONFIG_PATH_MAX];
  int32_t retval;

  if ((retval = CopyPathFromUser(nap, path, sizeof(path), (uintptr_t) pathname))) return -NACL_ABI_EINVAL;

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_unlink(path, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceUnlink(nap->cage_id, path, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysRename(struct NaClAppThread *natp, const char *oldpath, const char *newpath) {
  struct NaClApp *nap = natp->nap;
  char oldpathname[NACL_CONFIG_PATH_MAX];
  char newpathname[NACL_CONFIG_PATH_MAX];
  int32_t retval;

  if ((retval = CopyPathFromUser(nap, oldpathname, sizeof(oldpathname), (uintptr_t) oldpath))) return -NACL_ABI_EINVAL;

  if ((retval = CopyPathFromUser(nap, newpathname, sizeof(newpathname), (uintptr_t) newpath))) return -NACL_ABI_EINVAL;

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_rename(oldpathname, newpathname, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceRename(nap->cage_id, oldpathname, newpathname, retval, totaltime);
  #endif

  return retval;
}

int NaClSysCommonAddrRangeContainsExecutablePages(struct NaClApp *nap,
                                                  uintptr_t usraddr,
                                                  size_t length) {
  /*
   * NOTE: currently only trampoline and text region are executable,
   * and they are at the beginning of the address space, so this code
   * is fine.  We will probably never allow users to mark other pages
   * as executable; but if so, we will have to revisit how this check
   * is implemented.
   *
   * nap->static_text_end is a multiple of 4K, the memory protection
   * granularity.  Since this routine is used for checking whether
   * memory map adjustments / allocations -- which has 64K granularity
   * -- is okay, usraddr must be an allocation granularity value.  Our
   * callers (as of this writing) does this, but we truncate it down
   * to an allocation boundary to be sure.
   */
  UNREFERENCED_PARAMETER(length);
  usraddr = NaClTruncAllocPage(usraddr);
  return usraddr < nap->dynamic_text_end;
}

int NaClSysCommonAddrRangeInAllowedDynamicCodeSpace(struct NaClApp *nap,
                                                    uintptr_t usraddr,
                                                    size_t length) {
  uintptr_t usr_region_end = usraddr + length;

  if (usr_region_end < usraddr) {
    /* Check for unsigned addition overflow */
    return 0;
  }
  usr_region_end = NaClRoundAllocPage(usr_region_end);
  if (usr_region_end < usraddr) {
    /* 32-bit systems only, rounding caused uint32_t overflow */
    return 0;
  }
  return (nap->dynamic_text_start <= usraddr &&
          usr_region_end <= nap->dynamic_text_end);
}



static int32_t MunmapInternal(struct NaClApp *nap, uintptr_t sysaddr, size_t length) {
#if NACL_WINDOWS
  uintptr_t addr;
  uintptr_t endaddr = sysaddr + length;
  uintptr_t usraddr;
  for (addr = sysaddr; addr < endaddr; addr += NACL_MAP_PAGESIZE) {
    struct NaClVmmapEntry const *entry;
    uintptr_t                   page_num;
    uintptr_t                   offset;

    usraddr = NaClSysToUser(nap, addr);

    entry = NaClVmmapFindPage(&nap->mem_map, usraddr >> NACL_PAGESHIFT);
    if (!entry) {
      continue;
    }
    NaClLog(2, "NaClSysMunmap: addr 0x%08lx, desc 0x%08"NACL_PRIxPTR"\n",
            addr, (uintptr_t)entry->desc);

    page_num = usraddr - (entry->page_num << NACL_PAGESHIFT);
    offset = (uintptr_t) entry->offset + page_num;

    if (entry->desc &&
        offset < (uintptr_t) entry->file_size) {
      if (!UnmapViewOfFile((void *) addr)) {
        NaClLog(1, "MunmapInternal: UnmapViewOfFile failed to at addr"
                " 0x%08"NACL_PRIxPTR", error %d\n",
                addr, GetLastError());
      }
      /*
      * Fill the address space hole that we opened
      * with UnmapViewOfFile().
      */
      if (!VirtualAlloc((void *) addr, NACL_MAP_PAGESIZE, MEM_RESERVE,
                        PAGE_READWRITE)) {
        NaClLog(LOG_FATAL, "MunmapInternal: "
                "failed to fill hole with VirtualAlloc(), error %d\n",
                GetLastError());
      }
    } else {
      /*
       * Anonymous memory; we just decommit it and thus
       * make it inaccessible.
       */
      if (!VirtualFree((void *) addr,
                       NACL_MAP_PAGESIZE,
                       MEM_DECOMMIT)) {
        int error = GetLastError();
        NaClLog(LOG_FATAL,
                ("MunmapInternal: Could not VirtualFree MEM_DECOMMIT"
                 " addr 0x%08x, error %d (0x%x)\n"),
                addr, error, error);
      }
    }
    NaClVmmapRemove(&nap->mem_map,
                    usraddr >> NACL_PAGESHIFT,
                    NACL_PAGES_PER_MAP);
  }
#else /* NACL_WINDOWS */
  UNREFERENCED_PARAMETER(nap);
  NaClLog(3, "MunmapInternal(0x%08"NACL_PRIxPTR", 0x%"NACL_PRIxS")\n", sysaddr, length);
  /*
   * Overwrite current mapping with inaccessible, anonymous
   * zero-filled pages, which should be copy-on-write and thus
   * relatively cheap.  Do not open up an address space hole.
   */
  if (MAP_FAILED == (void *) lind_mmap((void *) sysaddr,
                              length,  
                              PROT_NONE,  
                              MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, 
                              -1, 
                              (off_t) 0,
                              nap->cage_id)) {
    NaClLog(2, "mmap to put in anonymous memory failed, errno = %d\n", errno);
    return -NaClXlateErrno(errno);
  }
  NaClVmmapRemove(&nap->mem_map,
                  NaClSysToUser(nap, sysaddr) >> NACL_PAGESHIFT,
                  length >> NACL_PAGESHIFT);
#endif /* NACL_WINDOWS */
  return 0;
}

/* Warning: sizeof(nacl_abi_off_t) != sizeof(off_t) on OSX */

// Lind (NR): we've removed any ability for mmap to map executable code for several reasons
// 1. It made removing the NaClDesc system much easier
// 2. It relieves us of any worry about a mechanism that obviously raises some security concerns
// 3. We don't rely on these settings to map the intial program, and we don't anticipate programs where we will neep to map code
//
// We've also removed the VMHole functions since they are specific to Windows 
int32_t NaClSysMmapIntern(struct NaClApp        *nap,
                          void                  *start,
                          size_t                length,
                          int                   prot,
                          int                   flags,
                          int                   d,
                          nacl_abi_off_t        offset) {
  int                         allowed_flags;
  uintptr_t                   usraddr;
  uintptr_t                   usrpage;
  uintptr_t                   sysaddr;
  uintptr_t                   endaddr;
  uintptr_t                   map_result;
  int                         holding_app_lock = 0;
  size_t                      alloc_rounded_length;
  unsigned long               topbits;
  unsigned int                mapbottom;
  int                         desc;
  int                         maxprot = PROT_RW;
  nacl_abi_off_t              entry_offset = ENTRY_OFFSET_NOFD;

  allowed_flags = (NACL_ABI_MAP_FIXED | NACL_ABI_MAP_SHARED
                   | NACL_ABI_MAP_PRIVATE | NACL_ABI_MAP_ANONYMOUS);

  usraddr = (uintptr_t) start;

  if ((flags & ~allowed_flags)) {
    NaClLog(2, "invalid mmap flags 0%o, ignoring extraneous bits\n", flags);
    flags &= allowed_flags;
  }

  if (prot & NACL_ABI_PROT_EXEC) {
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }

  /*
   * Starting address must be aligned to worst-case allocation
   * granularity.  (Windows.)
   */
  if (!NaClIsAllocPageMultiple(usraddr)) {
    NaClLog(2, "NaClSysMmap: address not allocation granularity aligned\n");
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  /*
   * Offset should be non-negative (nacl_abi_off_t is signed).  This
   * condition is caught when the file is stat'd and checked, and
   * offset is ignored for anonymous mappings.
   */
  if (offset < 0) {
    NaClLog(1,  /* application bug */
            "NaClSysMmap: negative file offset: %"NACL_PRIdNACL_OFF"\n",
            offset);
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  /*
   * And offset must be a multiple of the allocation unit.
   */
  if (!NaClIsAllocPageMultiple((uintptr_t) offset)) {
    NaClLog(1, "NaClSysMmap: file offset 0x%08"NACL_PRIxPTR" not multiple"
            " of allocation size\n",
            (uintptr_t)offset);
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }

  if (!length) {
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  alloc_rounded_length = NaClRoundAllocPage(length);
  if (alloc_rounded_length != length) {
    NaClLog(1, "NaClSysMmap: rounded length to 0x%"NACL_PRIxS"\n",
            alloc_rounded_length);
  }

 
  length = alloc_rounded_length;

  /*
   * Lock the addr space.
   */
  NaClXMutexLock(&nap->mu);

  holding_app_lock = 1;

  if (0 == (flags & NACL_ABI_MAP_FIXED)) {
    /*
     * The user wants us to pick an address range.
     */
    if (!usraddr) {
      /*
       * Pick a hole in addr space of appropriate size, anywhere.
       * We pick one that's best for the system.
       */
      usrpage = NaClVmmapFindMapSpace(&nap->mem_map,
                                      alloc_rounded_length >> NACL_PAGESHIFT);
      NaClLog(2, "NaClSysMmap: FindMapSpace: page 0x%05"NACL_PRIxPTR"\n",
              usrpage);
      if (!usrpage) {
        map_result = -NACL_ABI_ENOMEM;
        goto cleanup;
      }
      usraddr = usrpage << NACL_PAGESHIFT;
      NaClLog(2, "NaClSysMmap: new starting addr: 0x%08"NACL_PRIxPTR
              "\n", usraddr);
    } else {
      /*
       * user supplied an addr, but it's to be treated as a hint; we
       * find a hole of the right size in the app's address space,
       * according to the usual mmap semantics.
       */
      usrpage = NaClVmmapFindMapSpaceAboveHint(&nap->mem_map,
                                               usraddr,
                                               (alloc_rounded_length
                                                >> NACL_PAGESHIFT));
      NaClLog(2, "NaClSysMmap: FindSpaceAboveHint: page 0x%05"NACL_PRIxPTR"\n",
              usrpage);
      if (!usrpage) {
        NaClLog(2, "%s\n", "NaClSysMmap: hint failed, doing generic allocation");
        usrpage = NaClVmmapFindMapSpace(&nap->mem_map,
                                        alloc_rounded_length >> NACL_PAGESHIFT);
      }
      if (!usrpage) {
        map_result = -NACL_ABI_ENOMEM;
        goto cleanup;
      }
      usraddr = usrpage << NACL_PAGESHIFT;
      NaClLog(2, "NaClSysMmap: new starting addr: 0x%08"NACL_PRIxPTR"\n",
              usraddr);
    }
  }

  /*
   * Validate [usraddr, endaddr) is okay.
   */
  if (usraddr >= ((uintptr_t) 1 << nap->addr_bits)) {
    NaClLog(2,
            ("NaClSysMmap: start address (0x%08"NACL_PRIxPTR") outside address"
             " space\n"),
            usraddr);
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  endaddr = usraddr + alloc_rounded_length;
  if (endaddr < usraddr) {
    NaClLog(0,
            ("NaClSysMmap: integer overflow -- "
             "NaClSysMmap(0x%08"NACL_PRIxPTR",0x%"NACL_PRIxS",0x%x,0x%x,%d,"
             "0x%08"NACL_PRIxPTR"\n"),
            usraddr, length, prot, flags, d, (uintptr_t) offset);
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  /*
   * NB: we use > instead of >= here.
   *
   * endaddr is the address of the first byte beyond the target region
   * and it can equal the address space limit.  (of course, normally
   * the main thread's stack is there.)
   */
  if (endaddr > ((uintptr_t) 1 << nap->addr_bits)) {
    NaClLog(2,
            ("NaClSysMmap: end address (0x%08"NACL_PRIxPTR") is beyond"
             " the end of the address space\n"),
            endaddr);
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }

  if (NaClSysCommonAddrRangeContainsExecutablePages(nap, usraddr, length)) {
    NaClLog(2, "NaClSysMmap: region contains executable pages\n");
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }

  /*
   * Force NACL_ABI_MAP_FIXED, since we are specifying address in NaCl
   * app address space.
   */
  flags |= NACL_ABI_MAP_FIXED;

  /*
   * Turn off PROT_EXEC -- normal user mmapped pages should not be
   * executable.  This is primarily for the service runtime's own
   * bookkeeping -- prot is used in NaClVmmapAddWithOverwrite and will
   * be needed for remapping data pages on Windows if page protection
   * is set to PROT_NONE and back.
   *
   * NB: we've captured the notion of mapping executable memory for
   * dynamic library loading etc in mapping_code, so when we do map
   * text we will explicitly OR in NACL_ABI_PROT_EXEC as needed.
   */
  prot &= ~NACL_ABI_PROT_EXEC;

  /*
   * Exactly one of NACL_ABI_MAP_SHARED and NACL_ABI_MAP_PRIVATE is set.
   */
  if ((0 == (flags & NACL_ABI_MAP_SHARED)) ==
      (0 == (flags & NACL_ABI_MAP_PRIVATE))) {
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }

  //We don't check prot in mmap
  sysaddr = NaClUserToSys(nap, usraddr);

  /* [0, length) */
  if (length > 0) {


    if (flags & NACL_ABI_MAP_ANONYMOUS) {
      desc = -1;
    }
    else {
      desc = d;
      entry_offset = offset;
      maxprot = lind_fcntl_get(d, F_GETFL, nap->cage_id) & (NACL_ABI_PROT_READ | NACL_ABI_PROT_WRITE); // get and extract file protection flags
    }
    
    NaClLog(2, "NaClSysMmap: (,,0x%08" NACL_PRIxPTR ","
               "0x%08" NACL_PRIxS ",0x%x,0x%x,0x%08" NACL_PRIxPTR ")\n",
            sysaddr, length, prot, flags, (uintptr_t)offset);

    /* finally lets create the segment */
    topbits = (long)sysaddr & 0xffffffff00000000L;
    mapbottom = lind_mmap(sysaddr, length, prot, flags, desc, offset, nap->cage_id);


    /* If we return a value higher than 0xffffffffu - 256
     * we know that this is in fact a negative integer (an errno)
     * since due to alignment mmap cannot return an address in that range
     */
    if ((unsigned)mapbottom > (0xffffffffu - 256)) {
      errno = mapbottom;
      mapbottom = MAP_FAILED;
    }

    /* MAP_FAILED is -1, so if we get that as our bottom 32 bits, we
     * return a long -1 as our return value. Otherwise, combine the
     * top bits and bottom bits into our full return value.
     */
    map_result = (mapbottom == (unsigned int)-1 ? (unsigned long)-1L : topbits | (unsigned long)mapbottom);

    if ((unsigned int)-1 == map_result) {
      NaClLog(LOG_INFO,
              ("NaClSysMmap: "
               "mmap(0x%08" NACL_PRIxPTR ", %d" NACL_PRIxS ", "
               "0x%d, 0x%d)"
               " failed, errno %d.\n"),
              (uintptr_t)sysaddr, length, prot, flags,
              errno);
      return -NaClXlateErrno(errno);
    }
    if (map_result != sysaddr) {
      NaClLog(LOG_FATAL,
              ("NaClSysMmap: MAP_FIXED not fixed:"
               " returned 0x%08" NACL_PRIxPTR " instead of 0x%08" NACL_PRIxPTR "\n"),
              (uintptr_t)map_result,
              (uintptr_t)sysaddr);
    }
    /*
     * "Small" negative integers are errno values.  Larger ones are
     * virtual addresses.
     */
    if (NaClPtrIsNegErrno(&map_result)) {
      if ((uintptr_t) -NACL_ABI_E_MOVE_ADDRESS_SPACE == map_result) {
        NaClLog(LOG_FATAL,
                ("NaClSysMmap: Map failed, but we"
                 " cannot handle address space move, error %"NACL_PRIuS"\n"),
                (size_t) map_result);
      }
      /*
       * Propagate all other errors to user code.
       */
      goto cleanup;
    }
    if (map_result != sysaddr) {
      NaClLog(LOG_FATAL, "system mmap did not honor NACL_ABI_MAP_FIXED\n");
    }
  }
  /*
   * If we are mapping beyond the end of the file, we fill this space
   * with PROT_NONE pages.
   *
   * Windows forces us to expose a mixture of 64k and 4k pages, and we
   * expose the same mappings on other platforms.  For example,
   * suppose untrusted code requests to map 0x40000 bytes from a file
   * of extent 0x100.  We will create the following regions:
   *
   *       0-  0x100  A: Bytes from the file
   *   0x100- 0x1000  B: The rest of the 4k page is accessible but undefined
   *  0x1000-0x10000  C: The rest of the 64k page is inaccessible (PROT_NONE)
   * 0x10000-0x40000  D: Further 64k pages are also inaccessible (PROT_NONE)
   *
   * On Windows, a single MapViewOfFileEx() call creates A, B and C.
   * This call will not accept a size greater than 0x100, so we have
   * to create D separately.  The hardware requires B to be accessible
   * (whenever A is accessible), but Windows does not allow C to be
   * mapped as accessible.  This is unfortunate because it interferes
   * with how ELF dynamic linkers usually like to set up an ELF
   * object's BSS.
   */
  /* inaccessible: [length, alloc_rounded_length) */
  if (length < alloc_rounded_length) {
    /*
     * On Unix, this maps regions C and D as inaccessible.  On
     * Windows, it just maps region D; region C has already been made
     * inaccessible.
     */
    size_t map_len = alloc_rounded_length - length;
    map_result = MunmapInternal(nap, sysaddr + length, map_len);
    if (map_result != 0) {
      goto cleanup;
    }
  }

  if (alloc_rounded_length > 0) {
    NaClVmmapAddWithOverwrite(&nap->mem_map,
                              NaClSysToUser(nap, sysaddr) >> NACL_PAGESHIFT,
                              alloc_rounded_length >> NACL_PAGESHIFT,
                              prot,
                              maxprot,
                              flags,
                              NULL,
                              entry_offset,
                              length);
  }

  map_result = usraddr;

 cleanup:
  if (holding_app_lock) {
    NaClXMutexUnlock(&nap->mu);
  }

  /*
   * Check to ensure that map_result will fit into a 32-bit value. This is
   * a bit tricky because there are two valid ranges: one is the range from
   * 0 to (almost) 2^32, the other is from -1 to -4096 (our error range).
   * For a 32-bit value these ranges would overlap, but if the value is 64-bit
   * they will be disjoint.
   */
  if (map_result > UINT32_MAX
      && !NaClPtrIsNegErrno(&map_result)) {
    NaClLog(LOG_FATAL, "Overflow in NaClSysMmap: return address is "
                       "0x%"NACL_PRIxPTR"\n", map_result);
  }
  NaClLog(3, "NaClSysMmap: returning 0x%08"NACL_PRIxPTR"\n", map_result);

  return (int32_t) map_result;
}

int32_t NaClSysMmap(struct NaClAppThread  *natp,
                    void                  *start,
                    size_t                length,
                    int                   prot,
                    int                   flags,
                    int                   d,
                    nacl_abi_off_t        *offp) {
  struct NaClApp     *nap = natp->nap;
  int32_t            retval;
  volatile uintptr_t sysaddr;
  nacl_abi_off_t     offset;

  NaClLog(2, "Entered NaClSysMmap(0x%08"NACL_PRIxPTR",0x%"NACL_PRIxS","
          "0x%x,0x%x,%d,0x%08"NACL_PRIxPTR")\n",
          (uintptr_t) start, length, prot, flags, d, (uintptr_t)offp);

  if (!offp) {
    /*
     * This warning is really targetted towards trusted code,
     * especially tests that didn't notice the argument type change.
     * Unfortunatey, zero is a common and legitimate offset value, and
     * the compiler will not complain since an automatic type
     * conversion works.
     */
#ifdef  _DEBUG
    NaClLog(1, "NaClSysMmap: NULL pointer used"
            " for offset in/out argument\n");
#endif
    return -NACL_ABI_EINVAL;
  }

  //We don't check prot in mmap
  sysaddr = NaClUserToSysAddrRange(nap, (uintptr_t)offp, sizeof(offset));
  if (kNaClBadAddress == sysaddr) {
    NaClLog(2, "%s\n", "NaClSysMmap: offset in a bad untrusted memory location");
    retval = -NACL_ABI_EFAULT;
    goto cleanup;
  }
  offset = *(nacl_abi_off_t *)sysaddr;
  NaClLog(2, " offset = 0x%08"NACL_PRIxNACL_OFF"\n", offset);
  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  retval = NaClSysMmapIntern(nap, start, length, prot, flags, d, offset);

cleanup:

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceMmap(nap->cage_id, start, length, prot, flags, d, offset, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysMunmap(struct NaClAppThread  *natp,
                      void                  *start,
                      size_t                length) {
  struct NaClApp *nap = natp->nap;
  int32_t   retval = -NACL_ABI_EINVAL;
  uintptr_t sysaddr;
  int       holding_app_lock = 0;
  size_t    alloc_rounded_length;

  NaClLog(2, "Entered NaClSysMunmap(0x%08"NACL_PRIxPTR", "
          "0x%08"NACL_PRIxPTR", 0x%"NACL_PRIxS")\n",
          (uintptr_t) natp, (uintptr_t) start, length);

  if (!NaClIsAllocPageMultiple((uintptr_t) start)) {
    NaClLog(2, "%s\n", "start addr not allocation multiple");
    retval = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  if (!length) {
    /*
     * Without this check we would get the following inconsistent
     * behaviour:
     *  * On Linux, an mmap() of zero length yields a failure.
     *  * On Mac OS X, an mmap() of zero length returns no error,
     *    which would lead to a NaClVmmapUpdate() of zero pages, which
     *    should not occur.
     *  * On Windows we would iterate through the 64k pages and do
     *    nothing, which would not yield a failure.
     */
    retval = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  alloc_rounded_length = NaClRoundAllocPage(length);
  if (alloc_rounded_length != length) {
    length = alloc_rounded_length;
    NaClLog(2, "munmap: rounded length to 0x%"NACL_PRIxS"\n", length);
  }
  //Prot is explicit in munmap
  sysaddr = NaClUserToSysAddrRange(nap, (uintptr_t) start, length);
  if (kNaClBadAddress == sysaddr) {
    NaClLog(4, "munmap: region not user addresses\n");
    retval = -NACL_ABI_EFAULT;
    goto cleanup;
  }

  NaClXMutexLock(&nap->mu);

  holding_app_lock = 1;

  /*
   * User should be unable to unmap any executable pages.  We check here.
   */
  if (NaClSysCommonAddrRangeContainsExecutablePages(nap,
                                                    (uintptr_t) start,
                                                    length)) {
    NaClLog(2, "NaClSysMunmap: region contains executable pages\n");
    retval = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = MunmapInternal(nap, sysaddr, length);
cleanup:
  if (holding_app_lock) {
    NaClXMutexUnlock(&nap->mu);
  }

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceMunmap(nap->cage_id, sysaddr, length, retval, totaltime);
  #endif

  return retval;
}

#if NACL_WINDOWS
static int32_t MprotectInternal(struct NaClApp *nap,
                                uintptr_t sysaddr, size_t length, int prot) {
  uintptr_t addr;
  uintptr_t endaddr = sysaddr + length;
  uintptr_t usraddr;
  DWORD     flProtect;
  DWORD     flOldProtect;

  /*
   * VirtualProtect region cannot span allocations, all addresses must be
   * in one region of memory returned from VirtualAlloc or VirtualAllocEx.
   */
  for (addr = sysaddr; addr < endaddr; addr += NACL_MAP_PAGESIZE) {
    struct NaClVmmapEntry const *entry;
    uintptr_t                   page_num;
    uintptr_t                   offset;

    usraddr = NaClSysToUser(nap, addr);

    entry = NaClVmmapFindPage(&nap->mem_map, usraddr >> NACL_PAGESHIFT);
    if (!entry) {
      continue;
    }
    NaClLog(2, "MprotectInternal: addr 0x%08x, desc 0x%08"NACL_PRIxPTR"\n",
            addr, (uintptr_t) entry->desc);

    page_num = usraddr - (entry->page_num << NACL_PAGESHIFT);
    offset = (uintptr_t) entry->offset + page_num;

    if (!entry->desc) {
      flProtect = NaClflProtectMap(prot);

      /* Change the page protection */
      if (!VirtualProtect((void *) addr,
                          NACL_MAP_PAGESIZE,
                          flProtect,
                          &flOldProtect)) {
        int error = GetLastError();
        NaClLog(LOG_FATAL, "MprotectInternal: "
                "failed to change the memory protection with VirtualProtect,"
                " addr 0x%08x, error %d (0x%x)\n",
                addr, error, error);
        return -NaClXlateSystemError(error);
      }
    } else if (offset < (uintptr_t) entry->file_size) {
      nacl_off64_t  file_bytes;
      size_t        chunk_size;
      size_t        rounded_chunk_size;
      int           desc_flags;
      char const    *err_msg;

      desc_flags = (*NACL_VTBL(NaClDesc, entry->desc)->GetFlags)(entry->desc);
      NaClflProtectAndDesiredAccessMap(prot,
                                       (entry->flags
                                        & NACL_ABI_MAP_PRIVATE) != 0,
                                       (desc_flags & NACL_ABI_O_ACCMODE),
                                       /* flMaximumProtect= */ NULL,
                                       &flProtect,
                                       /* dwDesiredAccess= */ NULL,
                                       &err_msg);
      if (!flProtect) {
        /*
         * This shouldn't really happen since we already checked the address
         * space using NaClVmmapCheckExistingMapping, but better be safe.
         */
        NaClLog(1, "MprotectInternal: %s\n", err_msg);
      }

      file_bytes = entry->file_size - offset;
      chunk_size = MIN((size_t) file_bytes, NACL_MAP_PAGESIZE);
      rounded_chunk_size = NaClRoundPage(chunk_size);

      NaClLog(2, "VirtualProtect(0x%08x, 0x%"NACL_PRIxS", %x)\n",
              addr, rounded_chunk_size, flProtect);

      /* Change the page protection */
      if (!VirtualProtect((void *) addr,
                          rounded_chunk_size,
                          flProtect,
                          &flOldProtect)) {
        int error = GetLastError();
        NaClLog(LOG_FATAL, "MprotectInternal: "
                "failed to change the memory protection with VirtualProtect()"
                " addr 0x%08x, error %d (0x%x)\n",
                addr, error, error);
        return -NaClXlateSystemError(error);
      }
    }
  }

  return 0;
}
#else
static int32_t MprotectInternal(struct NaClApp *nap,
                                uintptr_t sysaddr, size_t length, int prot) {
  uintptr_t               addr;
  uintptr_t               usraddr;
  uintptr_t               last_page_num;
  int                     host_prot;
  struct NaClVmmapIter    iter;
  struct NaClVmmapEntry   *entry;

  host_prot = NaClProtMap(prot);

  usraddr = NaClSysToUser(nap, sysaddr);
  last_page_num = (usraddr + length) >> NACL_PAGESHIFT;

  for (NaClVmmapFindPageIter(&nap->mem_map,
                             usraddr >> NACL_PAGESHIFT,
                             &iter);
       !NaClVmmapIterAtEnd(&iter) &&
         (entry = NaClVmmapIterStar(&iter))->page_num < last_page_num;
       NaClVmmapIterIncr(&iter)) {
    size_t entry_len = entry->npages << NACL_PAGESHIFT;

    usraddr = entry->page_num << NACL_PAGESHIFT;
    //This conversion has explicit prot handling
    addr = NaClUserToSys(nap, usraddr);

    NaClLog(2, "MprotectInternal: "
            "addr 0x%08"NACL_PRIxPTR", desc 0x%08"NACL_PRIxPTR"\n",
            addr, (uintptr_t) entry->desc);

    if (!entry->desc && entry->offset == ENTRY_OFFSET_NOFD) { // with no nacl desc and an entry offset set to ENTRY_OFFSET_NOFD, this is anonymous
      if (mprotect((void *) addr, entry_len, host_prot)) {
        NaClLog(1, "MprotectInternal: "
                "mprotect on anonymous memory failed, errno = %d\n", errno);
        return -NaClXlateErrno(errno);
      }
    } else if (entry->offset < entry->file_size) {
      nacl_abi_off64_t  file_bytes;
      size_t            rounded_file_bytes;
      size_t            prot_len;

      file_bytes = entry->file_size - entry->offset;
      rounded_file_bytes = NaClRoundPage((size_t) file_bytes);
      prot_len = MIN(rounded_file_bytes, entry_len);

      if (mprotect((void *) addr, prot_len, host_prot)) {
        NaClLog(1, "MprotectInternal: "
                "mprotect on file-backed memory failed, errno = %d\n", errno);
        return -NaClXlateErrno(errno);
      }
    }
  }

  return 0;
}
#endif

int32_t NaClSysMprotectInternal(struct NaClApp  *nap,
                                uint32_t        start,
                                size_t          length,
                                int             prot) {
  int32_t     retval = -NACL_ABI_EINVAL;
  uintptr_t   sysaddr;
  int         holding_app_lock = 0;

  if (!NaClIsAllocPageMultiple((uintptr_t) start)) {
    NaClLog(2, "%s\n", "mprotect: start addr not allocation multiple");
    retval = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  length = NaClRoundAllocPage(length);
  //This function has explicit prot checking
  sysaddr = NaClUserToSysAddrRange(nap, (uintptr_t) start, length);
  if (kNaClBadAddress == sysaddr) {
    NaClLog(2, "%s\n", "mprotect: region not user addresses");
    retval = -NACL_ABI_EFAULT;
    goto cleanup;
  }
  if ((~(NACL_ABI_PROT_READ | NACL_ABI_PROT_WRITE) & prot)) {
    NaClLog(2, "%s\n", "mprotect: prot has other bits than PROT_{READ|WRITE}");
    retval = -NACL_ABI_EACCES;
    goto cleanup;
  }

  NaClXMutexLock(&nap->mu);

  holding_app_lock = 1;

  if (!NaClVmmapCheckExistingMapping(
           &nap->mem_map, NaClSysToUser(nap, sysaddr) >> NACL_PAGESHIFT,
           length >> NACL_PAGESHIFT, prot)) {
    NaClLog(4, "mprotect: no such region\n");
    retval = -NACL_ABI_EACCES;
    goto cleanup;
  }

  /*
   * User should be unable to change protection of any executable pages.
   */
  if (NaClSysCommonAddrRangeContainsExecutablePages(nap,
                                                    (uintptr_t) start,
                                                    length)) {
    NaClLog(2, "NaClSysMprotect: region contains executable pages\n");
    retval = -NACL_ABI_EACCES;
    goto cleanup;
  }

  retval = MprotectInternal(nap, sysaddr, length, prot);
  if (!retval &&
      !NaClVmmapChangeProt(&nap->mem_map,
                           NaClSysToUser(nap, sysaddr) >> NACL_PAGESHIFT,
                           length >> NACL_PAGESHIFT,
                           prot)) {
    retval = -NACL_ABI_EACCES;
  }
cleanup:
  if (holding_app_lock) {
    NaClXMutexUnlock(&nap->mu);
  }
  return retval;
}

int32_t NaClSysMprotect(struct NaClAppThread  *natp,
                        uint32_t              start,
                        size_t                length,
                        int                   prot) {
  struct NaClApp  *nap = natp->nap;

  NaClLog(3, "Entered NaClSysMprotect(0x%08"NACL_PRIxPTR", "
          "0x%08"NACL_PRIxPTR", 0x%"NACL_PRIxS", 0x%x)\n",
          (uintptr_t) natp, (uintptr_t) start, length, prot);

  if (!NaClAclBypassChecks) {
    return -NACL_ABI_EACCES;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  int32_t retval = NaClSysMprotectInternal(nap, start, length, prot);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceMprotect(nap->cage_id, start, length, prot, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysShmget(struct NaClAppThread  *natp,
                      int                   key,
                      size_t                size,
                      int                   shmflg) {
  struct NaClApp                *nap = natp->nap;
  int32_t                       retval;
  size_t                        alloc_rounded_size;

  NaClLog(2, "Entered NaClSysShmget(0x%08"NACL_PRIxPTR" , %d, %lu, %d)\n",
           (uintptr_t)natp, key, size, shmflg);

  alloc_rounded_size = NaClRoundAllocPage(size);
  if (alloc_rounded_size != size) {
    NaClLog(1, "NaClSysShmget: rounded size to 0x%"NACL_PRIxS"\n",
            alloc_rounded_size);
  }
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_shmget(key, alloc_rounded_size, shmflg, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceShmget(nap->cage_id, key, alloc_rounded_size, shmflg, retval, totaltime);
  #endif

  if (retval > 0) {
    if(retval >= FILE_DESC_MAX)
      NaClLog(LOG_FATAL, "NaClSysShmget: shmid returned by lind is too large!\n");
    if(shmflg & IPC_CREAT) {
      shmtable[retval].size = alloc_rounded_size;
      shmtable[retval].rmid = false;
      shmtable[retval].extant = true;
    } else {
      if(!shmtable[retval].extant)
        NaClLog(LOG_FATAL, "NaClSysShmget: shmid returned by lind does not exist!\n");
    }
  }

  return retval;
}

int32_t NaClSysShmat(struct NaClAppThread  *natp,
                     int                   shmid,
                     void                  *shmaddr,
                     int                   shmflg) {
  struct NaClApp                *nap = natp->nap;
  uintptr_t                     map_result;
  uintptr_t                     usraddr;
  uintptr_t                     usrpage;
  uintptr_t                     sysaddr;
  uintptr_t                     endaddr;
  int                           length;
  unsigned long                 topbits;
  unsigned int                  mapbottom;

  NaClLog(2, "Entered NaClSysShmat(0x%08"NACL_PRIxPTR" , %d, %lx, %d)\n",
           (uintptr_t)natp, shmid, (uintptr_t)shmaddr, shmflg);

  if((unsigned) shmid >= FILE_DESC_MAX || !shmtable[shmid].extant) {
    NaClLog(2, "NaClSysShmat: shmid invalid\n");
    return -NACL_ABI_EINVAL;
  }

  length = shmtable[shmid].size;
  if (!length) return -NACL_ABI_EINVAL;


  usraddr = (uintptr_t) shmaddr;

   /*
   * Starting address must be aligned to worst-case allocation
   * granularity. 
   */
  if (!NaClIsAllocPageMultiple(usraddr)) {
    NaClLog(2, "NaClSysShmat: address not allocation granularity aligned\n");
    return -NACL_ABI_EINVAL;
  }

  /* Lock the addr space. */
  NaClXMutexLock(&nap->mu);

  /* Address space calculations */
  if (!usraddr) {
    /*
      * Pick a hole in addr space of appropriate size, anywhere.
      * We pick one that's best for the system.
      */
    usrpage = NaClVmmapFindMapSpace(&nap->mem_map,
                                    length >> NACL_PAGESHIFT);
    NaClLog(2, "NaClSysShmat: FindMapSpace: page 0x%05"NACL_PRIxPTR"\n",
            usrpage);
    if (!usrpage) {
      map_result = -NACL_ABI_ENOMEM;
      goto cleanup;
    }
    usraddr = usrpage << NACL_PAGESHIFT;
    NaClLog(2, "NaClSysShmat: new starting addr: 0x%08"NACL_PRIxPTR
            "\n", usraddr);
  } else {
    /*
      * user supplied an addr, but it's to be treated as a hint; we
      * find a hole of the right size in the app's address space,
      * according to the usual mmap semantics.
      */
    usrpage = NaClVmmapFindMapSpaceAboveHint(&nap->mem_map,
                                              usraddr,
                                              (length
                                              >> NACL_PAGESHIFT));
    NaClLog(2, "NaClSysShmat: FindSpaceAboveHint: page 0x%05"NACL_PRIxPTR"\n",
            usrpage);
    if (!usrpage) {
      NaClLog(2, "%s\n", "NaClSysShmat: hint failed, doing generic allocation");
      usrpage = NaClVmmapFindMapSpace(&nap->mem_map,
                                      length >> NACL_PAGESHIFT);
    }
    if (!usrpage) {
      map_result = -NACL_ABI_ENOMEM;
      goto cleanup;
    }
    usraddr = usrpage << NACL_PAGESHIFT;
    NaClLog(2, "NaClSysShmat: new starting addr: 0x%08"NACL_PRIxPTR"\n",
            usraddr);
  }

  /* Validate [usraddr, endaddr) is okay. */
  if (usraddr >= ((uintptr_t) 1 << nap->addr_bits)) {
    NaClLog(2,
            ("NaClSysShmat: start address (0x%08"NACL_PRIxPTR") outside address"
             " space\n"),
            usraddr);
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  endaddr = usraddr + length;
  if (endaddr < usraddr) {
    NaClLog(0,
            ("NaClSysShmat: integer overflow -- "
             "NaClSysShmat(0x%08"NACL_PRIxPTR",%d"NACL_PRIxS",%d\n"),
            usraddr, length, shmid);
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  /*
   * NB: we use > instead of >= here.
   *
   * endaddr is the address of the first byte beyond the target region
   * and it can equal the address space limit.  (of course, normally
   * the main thread's stack is there.)
   */
  if (endaddr > ((uintptr_t) 1 << nap->addr_bits)) {
    NaClLog(2,
            ("NaClSysShmat: end address (0x%08"NACL_PRIxPTR") is beyond"
             " the end of the address space\n"),
            endaddr);
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }

  if (NaClSysCommonAddrRangeContainsExecutablePages(nap, usraddr, length)) {
    NaClLog(2, "NaClSysShmat: region contains executable pages\n");
    map_result = -NACL_ABI_EINVAL;
    goto cleanup;
  }

  // translate to sysaddr
  sysaddr = NaClUserToSys(nap, usraddr);

  NaClLog(4, ("NaClSysShmat: (,,0x%08"NACL_PRIxPTR","
               "%d"NACL_PRIxS",%d\n"), sysaddr, length, shmid);

  //By this point in execution, we should have picked a sysaddr,
  //so start_addr should and cannot be null, but we sanity check
  if(!sysaddr){
    NaClLog(LOG_FATAL,
            "NaClSysShmat: sysaddr cannot be NULL.\n");
  }

  /* finally lets create the segment */
  topbits = (long) sysaddr & 0xffffffff00000000L;
  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  mapbottom = lind_shmat(shmid, (void *) sysaddr, shmflg, nap->cage_id);

  /* If we return a value higher than 0xffffffffu - 256
   * we know that this is in fact a negative integer (an errno)
   * since due to alignment mmap cannot return an address in that range 
   */

  if ((unsigned) mapbottom > (0xffffffffu - 256)) {
    errno = mapbottom;
    mapbottom = MAP_FAILED;
  } 

  /* MAP_FAILED is -1, so if we get that as our bottom 32 bits, we 
   * return a long -1 as our return value. Otherwise, combine the 
   * top bits and bottom bits into our full return value.
   */
  map_result = (mapbottom == (unsigned int) -1 ? (unsigned long) -1L : topbits | (unsigned long) mapbottom);
  
  int prot = NACL_ABI_O_RDWR;
  if (shmflg & SHM_RDONLY) prot = NACL_ABI_O_RDONLY;

  if ((unsigned int) -1 == map_result) {
    NaClLog(LOG_INFO,
            ("NaClSysShmat: "
             "mmap(0x%08"NACL_PRIxPTR", %d"NACL_PRIxS", "
             "0x%d, 0x%d)"
             " failed, errno %d.\n"),
            (uintptr_t) sysaddr, length, prot, shmid,
            errno);
    return -NaClXlateErrno(errno);
  }
  if (map_result != sysaddr) {
    NaClLog(LOG_FATAL,
            ("NaClSysShmat: MAP_FIXED not fixed:"
             " returned 0x%08"NACL_PRIxPTR" instead of 0x%08"NACL_PRIxPTR"\n"),
            (uintptr_t) map_result,
            (uintptr_t) sysaddr);
  }

  if (length > 0) {
    NaClVmmapAddWithOverwriteAndShmid(&nap->mem_map,
                                      NaClSysToUser(nap, sysaddr) >> NACL_PAGESHIFT,
                                      length >> NACL_PAGESHIFT,
                                      prot,
                                      PROT_RW,
                                      NACL_ABI_MAP_SHARED | NACL_ABI_MAP_FIXED,
                                      shmid,
                                      NULL,
                                      0,
                                      length);

    shmtable[shmid].count++;
  }

  map_result = usraddr;

cleanup:
  NaClXMutexUnlock(&nap->mu);

  /*
   * Check to ensure that map_result will fit into a 32-bit value. This is
   * a bit tricky because there are two valid ranges: one is the range from
   * 0 to (almost) 2^32, the other is from -1 to -4096 (our error range).
   * For a 32-bit value these ranges would overlap, but if the value is 64-bit
   * they will be disjoint.
   */
  if (map_result > UINT32_MAX
      && !NaClPtrIsNegErrno(&map_result)) {
    NaClLog(LOG_FATAL, "Overflow in NaClSysShmat: return address is "
                       "0x%"NACL_PRIxPTR"\n", map_result);
  }
  NaClLog(3, "NaClSysShmat: returning 0x%08"NACL_PRIxPTR"\n", map_result);

  #ifdef TRACING
    long long endtime = gettimens();
    long long totaltime = endtime - starttime;
  NaClStraceShmat(nap->cage_id, shmid, (void *) sysaddr, shmflg, map_result,totaltime);
  #endif

  return map_result;     
}

int32_t NaClSysShmdt(struct NaClAppThread  *natp,
                     void                  *shmaddr) {
  struct NaClApp                *nap = natp->nap;
  int                           retval;
  int                           shmid;
  uintptr_t                     sysaddr;
  int                           length;

  NaClLog(2, "Entered NaClSysShmdt(0x%08"NACL_PRIxPTR" ,""0x%08"NACL_PRIxPTR")\n",
           (uintptr_t)natp, (uintptr_t)shmaddr); 

  if (!NaClIsAllocPageMultiple((uintptr_t) shmaddr)) {
    NaClLog(2, "%s\n", "start addr not allocation multiple");
    retval = -NACL_ABI_EINVAL;
    return retval;
  }

  //Prot is explicit in munmap/shmdt
  sysaddr = NaClUserToSysAddr(nap, (uintptr_t) shmaddr);
  if (kNaClBadAddress == sysaddr) {
    NaClLog(4, "shmdt: region not user addresses\n");
    retval = -NACL_ABI_EFAULT;
    return retval;
  }

  NaClXMutexLock(&nap->mu);

  /*
   * User should be unable to unmap any executable pages.  We check here.
   */
  if (NaClSysCommonAddrRangeContainsExecutablePages(nap,
                                                    (uintptr_t) shmaddr,
                                                    1)) {
    NaClLog(2, "NaClSysShmdt: region contains executable pages\n");
    retval = -NACL_ABI_EINVAL;
    goto cleanup;
  }
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  shmid = lind_shmdt((void *) sysaddr, nap->cage_id);
  if (shmid < 0) {
    retval = shmid;
    NaClLog(2, "shmdt failed, errno = %d\n", retval);
    goto cleanup;
  }

  if((unsigned) shmid >= FILE_DESC_MAX || !shmtable[shmid].extant)
      NaClLog(LOG_FATAL, "NaClSysShmdt: nonsense shmid returned by lind_shmdt!");

  length = shmtable[shmid].size;

  // When the shmid entry is freed, we decrement the shm refcount
  NaClVmmapRemove(&nap->mem_map,
                  NaClSysToUser(nap, sysaddr) >> NACL_PAGESHIFT,
                  length >> NACL_PAGESHIFT);
  retval = 0;

cleanup:
  NaClXMutexUnlock(&nap->mu);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceShmdt(nap->cage_id, (void *) sysaddr, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysShmctl(struct NaClAppThread        *natp,
                      int                         shmid,
                      int                         cmd,
                      struct lind_shmid_ds        *buf) {
  struct NaClApp                *nap = natp->nap;
  int32_t                       retval;
  struct lind_shmid_ds          *bufsysaddr;

  NaClLog(2, "Entered NaClSysShmctl(0x%08"NACL_PRIxPTR" , %d, %d ,""%p"NACL_PRIxPTR")\n",
           (uintptr_t)natp, shmid, cmd, (void *) buf);

  

  if((unsigned) shmid >= FILE_DESC_MAX || !shmtable[shmid].extant) {
    NaClLog(2, "NaClSysShmat: shmid invalid\n");
    return -NACL_ABI_EINVAL;
  }

  if (cmd == IPC_STAT) {
    bufsysaddr = (struct lind_shmid_ds*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, sizeof(*bufsysaddr), NACL_ABI_PROT_READ);
  } else bufsysaddr = NULL;

  if ((void*) kNaClBadAddress == bufsysaddr) {
    NaClLog(2, "NaClSysShmCtl could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_shmctl(shmid, cmd, bufsysaddr, nap->cage_id);

  if (retval < 0) {
    return retval;
  }

  if (cmd == IPC_RMID) {
    shmtable[shmid].rmid = true;
    if (!shmtable[shmid].count) clear_shmentry(shmid); // if we dont have any entries attached we can clear it now
  }

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceShmctl(nap->cage_id, shmid, cmd, (uintptr_t)bufsysaddr, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysSocketPair(struct NaClAppThread *natp,
                          int                  domain,
                          int                  type,
                          int                  protocol,
                          int                  *fds) {

  struct NaClApp          *nap = natp->nap;
  int                     lindfds[2];
  int32_t                 retval;

  NaClLog(2, "Cage %d Entered NaClSysSocketPair(0x%08"NACL_PRIxPTR", "
           "%d, %d, %d, %lx)\n",
           nap->cage_id, (uintptr_t)natp, domain, type, protocol, (uintptr_t)fds);

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_socketpair (domain, type, protocol, lindfds, nap->cage_id);

  if (!NaClCopyOutToUser(nap, (uintptr_t)fds, lindfds, sizeof(lindfds))) return -NACL_ABI_EFAULT;

  NaClLog(2, "NaClSysSocketPair: returning %d\n", retval);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceSocketPair(nap->cage_id, domain, type, protocol, lindfds, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysTlsInit(struct NaClAppThread  *natp,
                       uint32_t              thread_ptr) {
  int32_t   retval = -NACL_ABI_EINVAL;
  uintptr_t sys_tls;

  NaClLog(3,
          ("Entered NaClSysTlsInit(0x%08"NACL_PRIxPTR
           ", 0x%08"NACL_PRIxPTR")\n"),
          (uintptr_t) natp, (uintptr_t) thread_ptr);

  /* Verify that the address in the app's range and translated from
   * nacl module address to service runtime address - a nop on ARM
   */
  sys_tls = NaClUserToSysAddrRangeProt(natp->nap, thread_ptr, 4, NACL_ABI_PROT_WRITE
                                                               | NACL_ABI_PROT_READ);
  NaClLog(4,
          "NaClSysTlsInit: thread_ptr 0x%"NACL_PRIx32
          ", sys_tls 0x%"NACL_PRIxPTR"\n",
          thread_ptr, sys_tls);
  if (kNaClBadAddress == sys_tls) {
    retval = -NACL_ABI_EFAULT;
    goto cleanup;
  }

  NaClTlsSetTlsValue1(natp, thread_ptr);
  retval = 0;
cleanup:
  return retval;
}

int32_t NaClSysThreadCreate(struct NaClAppThread *natp,
                            void                 *prog_ctr,
                            uint32_t             stack_ptr,
                            uint32_t             thread_ptr,
                            uint32_t             second_thread_ptr) {
  struct NaClApp *nap = natp->nap;
  int32_t     retval = -NACL_ABI_EINVAL;
  uintptr_t   sys_tls;
  uintptr_t   sys_stack;

  NaClLog(3,
          ("Entered NaClSysThreadCreate(0x%08"NACL_PRIxPTR
           " pc=0x%08"NACL_PRIxPTR", sp=0x%08"NACL_PRIx32", thread_ptr=0x%08"
           NACL_PRIx32")\n"),
          (uintptr_t) natp, (uintptr_t) prog_ctr, stack_ptr, thread_ptr);

  if (!NaClIsValidJumpTarget(nap, (uintptr_t) prog_ctr)) {
    NaClLog(LOG_ERROR, "NaClSysThreadCreate: Bad function pointer\n");
    retval = -NACL_ABI_EFAULT;
    goto cleanup;
  }

  /* Align the stack pointer. */
  stack_ptr = ((stack_ptr + NACL_STACK_PAD_BELOW_ALIGN)
               & ~NACL_STACK_ALIGN_MASK) - NACL_STACK_PAD_BELOW_ALIGN
              - NACL_STACK_ARGS_SIZE;

  sys_stack = NaClUserToSysAddrProt(nap, stack_ptr, NACL_ABI_PROT_READ 
                                                 | NACL_ABI_PROT_WRITE);
  if (kNaClBadAddress == sys_stack) {
    NaClLog(LOG_ERROR, "bad stack\n");
    retval = -NACL_ABI_EFAULT;
    goto cleanup;
  }
  sys_tls = NaClUserToSysAddrRangeProt(nap, thread_ptr, 4, NACL_ABI_PROT_READ |
                                                           NACL_ABI_PROT_WRITE);
  if (kNaClBadAddress == sys_tls) {
    NaClLog(LOG_ERROR, "bad TLS pointer\n");
    retval = -NACL_ABI_EFAULT;
    goto cleanup;
  }

  NaClVmHoleWaitToStartThread(nap);

  retval = NaClCreateAdditionalThread(natp,
                                      nap,
                                      (uintptr_t) prog_ctr,
                                      sys_stack,
                                      thread_ptr,
                                      second_thread_ptr);

cleanup:
  return retval;
}

/*
 * This is not used on x86-64 and its functionality is replaced by
 * NaClGetTlsFastPath1 (see nacl_syscall_64.S).
 */
int32_t NaClSysTlsGet(struct NaClAppThread *natp) {
  return NaClTlsGetTlsValue1(natp);
}

int32_t NaClSysSecondTlsSet(struct NaClAppThread *natp,
                            uint32_t             new_value) {
  NaClTlsSetTlsValue2(natp, new_value);
  return 0;
}

/*
 * This is not used on x86-64 and its functionality is replaced by
 * NaClGetTlsFastPath2 (see nacl_syscall_64.S).
 */
int32_t NaClSysSecondTlsGet(struct NaClAppThread *natp) {
  return NaClTlsGetTlsValue2(natp);
}

int NaClSysThreadNice(struct NaClAppThread *natp,
                      int                  nice) {
  /* Note: implementation of nacl_thread_nice is OS dependent. */
  UNREFERENCED_PARAMETER(natp);
  return nacl_thread_nice(nice);
}

int32_t NaClSysMutexCreate(struct NaClAppThread *natp) {
  struct NaClApp       *nap = natp->nap;
  int32_t              retval = -NACL_ABI_EINVAL;

  NaClLog(3,
          ("Entered NaClSysMutexCreate(0x%08"NACL_PRIxPTR")\n"),
          (uintptr_t) natp);

  retval = lind_mutex_create(nap->cage_id);

  NaClLog(3,
          ("NaClSysMutexCreate(0x%08"NACL_PRIxPTR") = %d\n"),
          (uintptr_t) natp, retval);


  return retval;
}

int32_t NaClSysMutexLock(struct NaClAppThread  *natp,
                         int32_t               mutex_handle) {
  struct NaClApp        *nap = natp->nap;
  int32_t               retval = -NACL_ABI_EINVAL;

  NaClLog(2, "Entered NaClSysMutexLock(0x%08"NACL_PRIxPTR", %d)\n",
          (uintptr_t)natp, mutex_handle);

  retval = lind_mutex_lock(mutex_handle, nap->cage_id);


  return retval;
}

int32_t NaClSysMutexUnlock(struct NaClAppThread  *natp,
                           int32_t               mutex_handle) {
  struct NaClApp  *nap = natp->nap;
  int32_t         retval = -NACL_ABI_EINVAL;

  NaClLog(2, "Entered NaClSysMutexUnlock(0x%08"NACL_PRIxPTR", %d)\")",
          (uintptr_t)natp, mutex_handle);

  retval = lind_mutex_unlock(mutex_handle, nap->cage_id);

  return retval;
}

int32_t NaClSysMutexTrylock(struct NaClAppThread   *natp,
                            int32_t                 mutex_handle) {
  struct NaClApp  *nap = natp->nap;
  int32_t         retval = -NACL_ABI_EINVAL;

  NaClLog(2, "Entered NaClSysMutexTrylock(0x%08"NACL_PRIxPTR", %d)\n",
          (uintptr_t)natp, mutex_handle);

  retval = lind_mutex_trylock(mutex_handle, nap->cage_id);

  return retval;
}

int32_t NaClSysMutexDestroy(struct NaClAppThread   *natp,
                            int32_t                 mutex_handle) {
  struct NaClApp  *nap = natp->nap;
  int32_t         retval = -NACL_ABI_EINVAL;

  NaClLog(2, "Entered NaClSysMutexDestroy(0x%08"NACL_PRIxPTR", %d)\n",
          (uintptr_t)natp, mutex_handle);

  retval = lind_mutex_destroy(mutex_handle, nap->cage_id);
 
  return retval;
}

int32_t NaClSysCondCreate(struct NaClAppThread *natp) {
  struct NaClApp         *nap = natp->nap;
  int32_t                retval = -NACL_ABI_EINVAL;

  NaClLog(3,
          ("Entered NaClSysCondCreate(0x%08"NACL_PRIxPTR")\n"),
          (uintptr_t) natp);

  retval = lind_cond_create(nap->cage_id);

  NaClLog(2, "NaClSysCondCreate(0x%08"NACL_PRIxPTR") = %d\n",
           (uintptr_t)natp, retval);

  return retval;
}

int32_t NaClSysCondWait(struct NaClAppThread *natp,
                        int32_t              cond_handle,
                        int32_t              mutex_handle) {
  struct NaClApp  *nap = natp->nap;
  int32_t         retval = -NACL_ABI_EINVAL;

  NaClLog(2, "Entered NaClSysCondWait(0x%08"NACL_PRIxPTR", %d, %d)\n",
           (uintptr_t)natp, cond_handle, mutex_handle);

  retval = lind_cond_wait(cond_handle, mutex_handle, nap->cage_id);

  return retval;
}

int32_t NaClSysCondSignal(struct NaClAppThread *natp,
                          int32_t              cond_handle) {
  struct NaClApp  *nap = natp->nap;
  int32_t         retval = -NACL_ABI_EINVAL;

  NaClLog(2, "Entered NaClSysCondSignal(0x%08"NACL_PRIxPTR", %d)\n",
           (uintptr_t)natp, cond_handle);

  retval = lind_cond_signal(cond_handle, nap->cage_id);

  return retval;
}

int32_t NaClSysCondBroadcast(struct NaClAppThread  *natp,
                             int32_t               cond_handle) {
  struct NaClApp  *nap = natp->nap;
  int32_t         retval = -NACL_ABI_EINVAL;

  NaClLog(2, "Entered NaClSysCondBroadcast(0x%08"NACL_PRIxPTR", %d)\n",
          (uintptr_t)natp, cond_handle);

  retval = lind_cond_broadcast(cond_handle, nap->cage_id);
 
  return retval;
}

int32_t NaClSysCondDestroy(struct NaClAppThread  *natp,
                           int32_t               cond_handle) {
  struct NaClApp  *nap = natp->nap;
  int32_t         retval = -NACL_ABI_EINVAL;

  NaClLog(2, "Entered NaClSysCondDestroy(0x%08"NACL_PRIxPTR", %d)\n",
          (uintptr_t)natp, cond_handle);

  retval = lind_cond_destroy(cond_handle, nap->cage_id);


  return retval;
}

int32_t NaClSysCondTimedWaitAbs(struct NaClAppThread     *natp,
                                int32_t                  cond_handle,
                                int32_t                  mutex_handle,
                                struct nacl_abi_timespec *ts) {
  struct NaClApp           *nap = natp->nap;
  struct nacl_abi_timespec trusted_ts;

  NaClLog(2, "Entered NaClSysCondTimedWaitAbs(0x%08"NACL_PRIxPTR
           ", %d, %d, 0x%08"NACL_PRIxPTR")\n",
           (uintptr_t)natp, cond_handle, mutex_handle, (uintptr_t)ts);

  if (!NaClCopyInFromUser(nap, &trusted_ts, (uintptr_t) ts, sizeof(trusted_ts))) { return -NACL_ABI_EFAULT; }

  int ret = lind_cond_timedwait(cond_handle, mutex_handle, (struct timespec*) &trusted_ts, nap->cage_id);

  return ret;
}

int32_t NaClSysSemInit(struct NaClAppThread *natp,
                         uint32_t             sem,
                         int32_t              pshared,
                         int32_t              value) {
  struct NaClApp  *nap = natp->nap;
  NaClLog(2, "Entered NaClSysSemInit(0x%08"NACL_PRIxPTR
           ", %d, %d, %d\n",
           (uintptr_t)natp, sem, pshared, value);
  int retval = lind_sem_init(sem, pshared, value, nap->cage_id);

  return retval;
}

int32_t NaClSysSemWait(struct NaClAppThread *natp,
                       uint32_t              sem) {
  struct NaClApp  *nap = natp->nap;
  NaClLog(2, "Entered NaClSysSemWait(0x%08"NACL_PRIxPTR
           ", %d\n",
           (uintptr_t)natp, sem);
  int retval = lind_sem_wait(sem, nap->cage_id);

  return retval;
}

int32_t NaClSysSemTryWait(struct NaClAppThread *natp,
                            uint32_t              sem) {
  struct NaClApp  *nap = natp->nap;
  NaClLog(2, "Entered NaClSysSemTryWait(0x%08"NACL_PRIxPTR
           ", %d\n",
           (uintptr_t)natp, sem);
  int retval = lind_sem_trywait(sem, nap->cage_id);

  return retval;
}

int32_t NaClSysSemTimedWait(struct NaClAppThread *natp,
                            uint32_t              sem,
                            struct nacl_abi_timespec *abs) {
  struct NaClApp  *nap = natp->nap;
  struct nacl_abi_timespec trusted_abs;

  NaClLog(2, "Entered NaClSysSemTimedWait(0x%08"NACL_PRIxPTR
           ", %d, 0x%08"NACL_PRIxPTR")\n",
           (uintptr_t)natp, sem, (uintptr_t)abs);

  if (!NaClCopyInFromUser(nap, &trusted_abs, (uintptr_t) abs, sizeof(trusted_abs))) { return -NACL_ABI_EFAULT; }

  int retval = lind_sem_timedwait(sem, (struct timespec*) &trusted_abs, nap->cage_id);

  return retval;
}

int32_t NaClSysSemPost(struct NaClAppThread *natp,
                       uint32_t              sem) {
  struct NaClApp  *nap = natp->nap;
  NaClLog(2, "Entered NaClSysSemPost(0x%08"NACL_PRIxPTR
           ", %d\n",
           (uintptr_t)natp, sem);

  int retval = lind_sem_post(sem, nap->cage_id);

  return retval;
}

int32_t NaClSysSemDestroy(struct NaClAppThread *natp,
                          uint32_t              sem) {
  struct NaClApp  *nap = natp->nap;
  NaClLog(2, "Entered NaClSysSemDestroy(0x%08"NACL_PRIxPTR
           ", %d\n",
           (uintptr_t)natp, sem);
  int retval = lind_sem_destroy(sem, nap->cage_id);

  return retval;
}

int32_t NaClSysSemGetValue(struct NaClAppThread *natp,
                           uint32_t              sem,
                           int32_t              *sval) {
  struct NaClApp  *nap = natp->nap;
  int lindsval;

  NaClLog(2, "Entered NaClSysSemGetvalue(0x%08"NACL_PRIxPTR
           ", %d\n",
           (uintptr_t)natp, sem);

  lindsval = lind_sem_getvalue(sem, nap->cage_id);

  // if we return a value, thats the sem value, lets set the user sval to it and return 0
  // otherwise return the errno
  if (lindsval > 0) {
    *sval = (int32_t)lindsval;
    lindsval = 0;
  } 

  return lindsval;
}

int32_t NaClSysNanosleep(struct NaClAppThread     *natp,
                         struct nacl_abi_timespec *req,
                         struct nacl_abi_timespec *rem) {
  struct NaClApp            *nap = natp->nap;
  struct nacl_abi_timespec  t_sleep;
  struct nacl_abi_timespec  t_rem;
  struct nacl_abi_timespec  *remptr;
  int                       retval = -NACL_ABI_EINVAL;

  NaClLog(2, "Entered NaClSysNanosleep(0x%08"NACL_PRIxPTR
           ", 0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR"x)\n",
           (uintptr_t)natp, (uintptr_t)req, (uintptr_t)rem);

  /* do the check before we sleep */
  if (rem && kNaClBadAddress ==
      NaClUserToSysAddrRangeProt(nap, (uintptr_t) rem, sizeof(*rem), NACL_ABI_PROT_READ)) {
    retval = -NACL_ABI_EFAULT;
    goto cleanup;
  }

  if (!NaClCopyInFromUser(nap, &t_sleep,
                          (uintptr_t) req, sizeof(t_sleep))) {
    retval = -NACL_ABI_EFAULT;
    goto cleanup;
  }

  remptr = !rem ? NULL : &t_rem;

  /*
   * We assume that we do not need to normalize the time request values.
   *
   * If bogus values can cause the underlying OS to get into trouble,
   * then we need more checking here.
   */
  NaClLog(2, "NaClSysNanosleep(time = %"NACL_PRId64".%09"NACL_PRId64" S)\n",
          (int64_t) t_sleep.tv_sec, (int64_t) t_sleep.tv_nsec);
  retval = NaClNanosleep(&t_sleep, remptr);
  NaClLog(2, "NaClNanosleep returned %d\n", retval);

  if (-NACL_ABI_EINTR == retval && rem && !NaClCopyOutToUser(nap, (uintptr_t)rem, remptr, sizeof(*remptr))) {
    NaClLog(1, "%s\n", "NaClSysNanosleep: check rem failed at copyout\n");
  }

cleanup:
  NaClLog(2, "%s\n", "nanosleep done.");

  return retval;
}

int32_t NaClSysSchedYield(struct NaClAppThread *natp) {
  UNREFERENCED_PARAMETER(natp);
  NaClThreadYield();

  

  return 0;
}

int32_t NaClSysExceptionHandler(struct NaClAppThread *natp,
                                uint32_t             handler_addr,
                                uint32_t             old_handler) {
  struct NaClApp *nap = natp->nap;
  int32_t rv = -NACL_ABI_EINVAL;

  if (!nap->enable_exception_handling) {
    rv = -NACL_ABI_ENOSYS;
    goto no_lock_exit;
  }
  if (!NaClIsValidJumpTarget(nap, handler_addr)) {
    rv = -NACL_ABI_EFAULT;
    goto no_lock_exit;
  }
  NaClXMutexLock(&nap->exception_mu);

  /*
   * This needs to be done while holding the lock so that we don't
   * start two Windows debug exception handlers.
   */
  if (handler_addr != 0) {
    if (!NaClDebugExceptionHandlerEnsureAttached(nap)) {
      rv = -NACL_ABI_ENOSYS;
      goto unlock_exit;
    }
  }

  if (old_handler &&
      !NaClCopyOutToUser(nap, (uintptr_t) old_handler,
                         &nap->exception_handler,
                         sizeof(nap->exception_handler))) {
    rv = -NACL_ABI_EFAULT;
    goto unlock_exit;
  }
  nap->exception_handler = handler_addr;
  rv = 0;
 unlock_exit:
  NaClXMutexUnlock(&nap->exception_mu);
 no_lock_exit:
 
  return rv;
}

int32_t NaClSysExceptionStack(struct NaClAppThread *natp,
                              uint32_t             stack_addr,
                              uint32_t             stack_size) {
  if (!natp->nap->enable_exception_handling) {
    return -NACL_ABI_ENOSYS;
  }
  if (kNaClBadAddress == NaClUserToSysAddrNullOkayProt(natp->nap,
                                                       stack_addr + stack_size,
                                                       NACL_ABI_PROT_READ | 
                                                       NACL_ABI_PROT_WRITE)) {
    return -NACL_ABI_EINVAL;
  }
  natp->exception_stack = stack_addr + stack_size;

  return 0;
}

int32_t NaClSysExceptionClearFlag(struct NaClAppThread *natp) {
  if (!natp->nap->enable_exception_handling) {
    return -NACL_ABI_ENOSYS;
  }
  natp->exception_flag = 0;

  return 0;
}

int32_t NaClSysTestInfoLeak(struct NaClAppThread *natp) {
#if NACL_ARCH(NACL_BUILD_ARCH) == NACL_x86
  /*
   * Put some interesting bits into the x87 and SSE registers.
   */
  union fxsave {
    char buf[512];
    struct {
      uint16_t fcw;
      uint16_t fsw;
      uint16_t ftw;
      uint16_t fop;
      union {
        struct {
          uint64_t rip;
          uint64_t rdp;
        } x64;
        struct {
          uint32_t fpu_ip;
          uint32_t cs;
          uint32_t fpu_dp;
          uint32_t ds;
        } ia32;
      } bitness;
      uint32_t mxcsr;
      uint32_t mxcsr_mask;
      struct {
        uint8_t st[10];
        uint8_t reserved[6];
      } st_space[8];
      uint32_t xmm_space[64];
    } fxsave;
  };

  static const char tenbytes[10] = "SecretBits";
  static const char manybytes[256] =
      "Highly sensitive information must not be leaked to untrusted code!\n"
      "xyzzy\nplugh\nYou are likely to be eaten by a grue.\n"
      "When in the Course of human events it becomes necessary for one people"
      " to dissolve the political bands which have connected them with ...\n";

# ifdef __GNUC__
  union fxsave u __attribute__((aligned(16)));
# elif NACL_WINDOWS
  __declspec(align(16)) union fxsave u;
# else
#  error Unsupported platform
# endif

  int i;

# ifdef __GNUC__
  __asm__("fxsave %0" : "=m" (u));
# elif NACL_WINDOWS
#  if NACL_BUILD_SUBARCH == 64
  NaClDoFxsave(&u);
#  else
  __asm {
    fxsave u
  };
#  endif
# else
# error Unsupported platform
# endif

  for (i = 0; i < 8; ++i)
    memcpy(&u.fxsave.st_space[i], tenbytes, sizeof(tenbytes));

  memcpy(u.fxsave.xmm_space, manybytes, sizeof(u.fxsave.xmm_space));

  /*
   * Set the MXCSR to an unlikely (but valid) value: all valid bits set.
   * The mask is provided by the hardware to say which bits can be set
   * (all others are reserved).  The untrusted test code (in
   * tests/infoleak/test_infoleak.c) sets MXCSR to zero before
   * making this system call so this value ensures that the test
   * actually verifies the behavior of the syscall return path.
   */
  u.fxsave.mxcsr = u.fxsave.mxcsr_mask;

# ifdef __GNUC__
  __asm__ volatile("fxrstor %0" :: "m" (u));
# elif NACL_WINDOWS
#  if NACL_BUILD_SUBARCH == 64
  NaClDoFxrstor(&u);
#  else
  __asm {
    fxrstor u
  };
#  endif
# else
# error Unsupported platform
# endif

#elif NACL_ARCH(NACL_BUILD_ARCH) == NACL_arm
  /*
   * Put some interesting bits into the VFP registers.
   */

  static const char manybytes[64] =
      "Sensitive information must not be leaked to untrusted code!!!!\n";

  __asm__ volatile("vldm %0, {d0-d7}" :: "r" (manybytes) :
                   "d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7");
  __asm__ volatile("fmxr fpscr, %0" :: "r" (0xdeadbeef) : "vfpcc");

#endif

  UNREFERENCED_PARAMETER(natp);

  return -NACL_ABI_ENOSYS;
}

/*
 * This syscall is intended for testing NaCl's support for Breakpad
 * crash reporting inside Chromium.  When
 * http://code.google.com/p/nativeclient/issues/detail?id=579 is
 * addressed, we might put this syscall behind a flag.  Until then,
 * untrusted code can trigger Breakpad-reported crashes inside
 * syscalls, so there is no benefit to restricting this syscall.
 */
int32_t NaClSysTestCrash(struct NaClAppThread *natp, int crash_type) {
  /*
   * Despite being volatile, the Apple system compiler, llvm-gcc, still
   * optimizes the null pointer dereference into an illegal instruction when
   * written as a one-liner. That interferes with tests that expect precisely
   * a SIGSEGV, because they'll see a SIGILL instead.
   */
  volatile int *volatile p = 0;
  UNREFERENCED_PARAMETER(natp);

  switch (crash_type) {
    case NACL_TEST_CRASH_MEMORY:
      *p = 0;
      break;
    case NACL_TEST_CRASH_LOG_FATAL:
      NaClLog(LOG_FATAL, "NaClSysTestCrash: This is a test error\n");
      break;
    case NACL_TEST_CRASH_CHECK_FAILURE:
      CHECK(0);
      break;
  }

  return -NACL_ABI_EINVAL;
}

int32_t NaClSysGetTimeOfDay(struct NaClAppThread      *natp,
                            struct nacl_abi_timeval   *tv,
                            struct nacl_abi_timezone  *tz) {
  int                     retval;
  struct nacl_abi_timeval now;

  UNREFERENCED_PARAMETER(tz);

  NaClLog(4,
          ("Entered NaClSysGetTimeOfDay(%08"NACL_PRIxPTR
           ", 0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR")\n"),
          (uintptr_t) natp, (uintptr_t) tv, (uintptr_t) tz);

  /*
   * tz is not supported in linux, nor is it supported by glibc, since
   * tzset(3) and the zoneinfo file should be used instead.
   *
   * TODO(bsy) Do we make the zoneinfo directory available to
   * applications?
   */

  retval = NaClGetTimeOfDay(&now);

#if !NACL_WINDOWS
  /*
   * Coarsen the time to the same level we get on Windows -
   * 10 microseconds.
   */
  if (!NaClHighResolutionTimerEnabled()) {
    now.nacl_abi_tv_usec = (now.nacl_abi_tv_usec / 10) * 10;
  }
#endif
  CHECK(now.nacl_abi_tv_usec >= 0);
  CHECK(now.nacl_abi_tv_usec < NACL_MICROS_PER_UNIT);
  if (!NaClCopyOutToUser(natp->nap, (uintptr_t)tv, &now, sizeof(now))) {
    return -NACL_ABI_EFAULT;
  }
  return 0;
}

/* convenience typedef */
typedef int clock_func(nacl_clockid_t, struct nacl_abi_timespec *);

static int NaClIsValidClockId(int clk_id) {
  int ret = 0;
  switch (clk_id) {
  case NACL_ABI_CLOCK_REALTIME:
  case NACL_ABI_CLOCK_MONOTONIC:
  case NACL_ABI_CLOCK_PROCESS_CPUTIME_ID:
  case NACL_ABI_CLOCK_THREAD_CPUTIME_ID:
    ret = 1;
  }
  return ret;
}

int32_t NaClSysClockGetCommon(struct NaClAppThread  *natp,
                              int                   clk_id,
                              uint32_t              ts_addr,
                              clock_func            *time_func) {
  struct NaClApp            *nap = natp->nap;
  int                       retval = -NACL_ABI_EINVAL;
  struct nacl_abi_timespec  out_buf;

  if (!NaClIsValidClockId(clk_id)) {
    goto done;
  }
  retval = time_func((nacl_clockid_t) clk_id, &out_buf);
  if (!retval && !NaClCopyOutToUser(nap, (uintptr_t)ts_addr, &out_buf, sizeof(out_buf))) {
    retval = -NACL_ABI_EFAULT;
  }

 done:

  return retval;
}

int32_t NaClSysClockGetRes(struct NaClAppThread *natp,
                           int                  clk_id,
                           uint32_t             tsp) {
  return NaClSysClockGetCommon(natp, clk_id, (uintptr_t) tsp, NaClClockGetRes);
}

int32_t NaClSysClockGetTime(struct NaClAppThread  *natp,
                            int                   clk_id,
                            uint32_t              tsp) {
  return NaClSysClockGetCommon(natp, clk_id, (uintptr_t) tsp, NaClClockGetTime);
}

int32_t NaClSysPipe2(struct NaClAppThread  *natp, uint32_t *pipedes, int flags) {
  struct NaClApp *nap = natp->nap;
  int32_t ret = 0;
  int lindfds[2];

  /* Attempt lind pipe RPC. Return lind pipe fds, if not return NaCl Error */
  ret = lind_pipe2(lindfds, flags, nap->cage_id);

  /* copy out NaCl fds */
  if (!NaClCopyOutToUser(nap, (uintptr_t)pipedes, lindfds, sizeof(lindfds))) return -NACL_ABI_EFAULT;

  return ret;
}

int32_t NaClSysPipe(struct NaClAppThread *natp, uint32_t *pipedes) {
  return NaClSysPipe2(natp, pipedes, 0);
}

int32_t NaClSysFork(struct NaClAppThread *natp) {
  struct NaClApp *nap = natp->nap;
  struct NaClApp *nap_child = 0;
  char **child_argv = 0;
  int child_argc = 0;
  int ret = -NACL_ABI_ENOMEM;

  NaClLog(1, "%s\n", "[NaClSysFork] NaCl fork starts!");

  /* set up new "child" NaClApp */
  NaClLogThreadContext(natp);
  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  /* get new id and setup new cage in safeposix */
  NaClXMutexLock(&nap->mu); 
  int child_cage_id = INIT_PROCESS_NUM + ++fork_num;
  lind_fork(child_cage_id, nap->cage_id); 

  nap_child = NaClChildNapCtor(natp->nap, child_cage_id, THREAD_LAUNCH_FORK);

  nap_child->argc = nap->argc;
  nap_child->argv = calloc((nap_child->argc + 1), sizeof(char*));
  for (int i = 0; i < nap_child->argc; i++) nap_child->argv[i] = strdup(nap->argv[i]);
  nap_child->binary = strdup(nap->binary);

  child_argc = nap_child->argc;
  child_argv = nap_child->argv;
  nap_child->running = 0;
  ret = child_cage_id;
  NaClXMutexUnlock(&nap->mu);

  /* start fork thread */
  if (!NaClCreateThread(natp, nap_child, child_argc, child_argv, nap_child->clean_environ)) {
    NaClLog(1, "%s\n", "[NaClSysFork] forking program failed!");
    ret = -NACL_ABI_ENOMEM;

    /* exit failed process in safeposix */
    lind_exit(EXIT_FAILURE, child_cage_id);
    goto fail;
  }

  /* success */
  
  NaClLog(1, "[fork_num = %u, child = %u, parent = %u]\n", fork_num, nap_child->cage_id, nap->cage_id);

fail:

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceFork(nap->cage_id, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysExecve(struct NaClAppThread *natp, char const *path, char *const *argv, char *const *envp) {
  struct NaClApp *nap = natp->nap;
  struct NaClEnvCleanser env_cleanser = {0};
  uint32_t *sys_envp_ptr = { NULL };
  char **new_envp = 0;
  int new_envc = 0;
  int ret = -NACL_ABI_ENOMEM;

  NaClLog(1, "%s\n", "[NaClSysExecve] NaCl execve() starts!");


  /* Make sys_envp_ptr a NULL array if we were passed NULL by EXECV */
  if (envp) {
    sys_envp_ptr = (uint32_t*)NaClUserToSysAddrProt(nap, (uintptr_t)envp, NACL_ABI_PROT_READ);
    int not_byte_aligned = (uintptr_t)sys_envp_ptr % 8;
    if (not_byte_aligned || (kNaClBadAddress == 0)) {
      NaClLog(2, "NaClSysExecve could not translate environment address, returning %d\n", -NACL_ABI_EFAULT);
      ret = -NACL_ABI_EFAULT;
      return ret;
    }
  }
  
  /* set up environment, only do this if we initially were passed an environment*/
  NaClEnvCleanserCtor(&env_cleanser, 0);
  if (envp) {
    /* Count amount of env from acquired NaCl pointer */
    uint32_t *envcountptr = sys_envp_ptr;
    while (envcountptr[new_envc] != 0) {
      new_envc++;
    }
    new_envp = calloc(new_envc + 1, sizeof(*new_envp));
    if (!new_envp) {
      NaClLog(LOG_ERROR, "%s\n", "Failed to allocate new_envv");
      NaClEnvCleanserDtor(&env_cleanser);
      goto fail;
    }
    for (int i = 0; i < new_envc; i++) {
      char *env = (void *)NaClUserToSysAddrProt(nap, (uintptr_t)sys_envp_ptr[i], NACL_ABI_PROT_READ);
      env = (uintptr_t)env == kNaClBadAddress ? 0 : env;
      if (!env) {
        new_envp[i] = NULL;
        break;
      }
      else {
        int envsize = NACL_ENV_PREFIX_LENGTH + strlen(env) + 1;
        new_envp[i] = calloc(envsize, sizeof(char));
        snprintf(new_envp[i], envsize, "%s%s", NACL_ENV_PREFIX, env);
      } 
    }
  }
  new_envp[new_envc] = NULL;

  /* We've already cleaned the native environment, so just supply extra args from this syscall */
  if (!NaClEnvCleanserInit(&env_cleanser, (char const *const *)new_envp, 0)) {
    NaClLog(LOG_ERROR, "%s\n", "Failed to initialize environment cleanser");
    NaClEnvCleanserDtor(&env_cleanser);
    goto fail;
  }

  nap->clean_environ = NaClEnvCleanserEnvironment(&env_cleanser);

  ret = NaClSysExecv(natp, path, argv);

fail:
  for (char **pp = new_envp; pp && *pp; pp++) {
    free(*pp);
  }
  free(new_envp);

 

  return ret; 
}
int32_t NaClSysExecv(struct NaClAppThread *natp, char const *path, char *const *argv) {
  struct NaClApp *nap = natp->nap;
  struct NaClApp *nap_child = 0;
  char *sys_pathname;
  uint32_t *sys_argv_ptr;
  char *binary; 
  char **child_argv = 0;
  char **new_argv = 0;
  int child_argc = 0;
  int new_argc = 0;
  int ret = -NACL_ABI_ENOMEM;
  void *dyncode_child;
  size_t dyncode_size;
  size_t dyncode_npages;
  size_t tramp_size;
  size_t tramp_npages;
  uintptr_t dyncode_pnum_child;
  uintptr_t parent_start_addr;
  uintptr_t child_start_addr;
  uintptr_t tramp_pnum;
  long long           starttime = 0;
  long long           endtime = 0;
  long long           totaltime = 0;

  #ifdef TRACING
  starttime = gettimens();
  #endif

  /* Convert pathname from user path, set binary */
  sys_pathname = (char *)  NaClUserToSysAddrProt(nap, (uintptr_t) path, NACL_ABI_PROT_READ);
  binary = sys_pathname ? strdup(sys_pathname) : NULL;

  /* 
    Convert to a Sys Pointer for argv** 
    We need to turn these to uint32_t pointers for now because these are still NaCl pointers
    We'll convert to a char** later.
  */
  sys_argv_ptr = (uint32_t*)NaClUserToSysAddrProt(nap, (uintptr_t)argv, NACL_ABI_PROT_READ);

  int not_byte_aligned = (uintptr_t)sys_argv_ptr % 8;
  if (not_byte_aligned || ((void *) kNaClBadAddress == sys_argv_ptr)) {
    NaClLog(2, "NaClSysExecv could not translate argv address, returning %d\n", -NACL_ABI_EFAULT);
    ret = -NACL_ABI_EFAULT;
    return ret;
  }

  /* set up argv and argc */
  if (!sys_argv_ptr) {
    NaClLog(LOG_ERROR, "%s\n", "Passed a NULL pointer in argp");
    goto fail;
  }
  
  /* Count amount of args from acquired NaCl pointer */
  uint32_t *argcountptr = sys_argv_ptr;
  while (argcountptr[new_argc] != 0) {
    new_argc++;
  }

  new_argv = calloc(new_argc + 1, sizeof(*new_argv));
  if (!new_argv) {
    NaClLog(LOG_ERROR, "%s\n", "Failed to allocate new_argv");
    goto fail;
  }
  for (int i = 0; i < new_argc; i++) {
    char *arg = (void *)NaClUserToSysAddrProt(nap, (uintptr_t)sys_argv_ptr[i], NACL_ABI_PROT_READ);
    arg = (uintptr_t)arg == kNaClBadAddress ? 0 : arg;
    new_argv[i] = arg ? strdup(arg) : 0;
    if (!arg) {
      break;
    }
  }
  new_argv[new_argc] = 0;

  /* set up child args */
  child_argc = new_argc + 3;
  child_argv = calloc(child_argc + 1, sizeof(*child_argv));
  if (!child_argv) {
    NaClLog(LOG_ERROR, "%s\n", "Failed to allocate child_argv");
    goto fail;
  }
  child_argv[0] = strdup("NaClMain");
  child_argv[1] = strdup("--library-path");
  child_argv[2] = strdup("/lib/glibc");
  for (int i = 0; i < new_argc; i++) {
    child_argv[i + 3] = new_argv[i] ? strdup(new_argv[i]) : NULL;
  }

  /* initialize child from parent state */
  NaClLogThreadContext(natp);
  int child_cage_id = INIT_PROCESS_NUM + ++fork_num;

  /* Copy fd table in SafePOSIX */
  NaClXMutexLock(&nap->mu);
  NaClLog(2, "Copying fd table in SafePOSIX\n");

  lind_exec(child_cage_id, nap->cage_id);

  nap_child = NaClChildNapCtor(nap, child_cage_id, THREAD_LAUNCH_EXEC);
  nap_child->running = 0;
  nap_child->in_fork = 0;

  /* add arguments to child nap */
  child_argv[child_argc] = NULL;
  nap_child->argc = child_argc;
  nap_child->argv = child_argv;
  if (binary) {
    free(nap_child->argv[3]);
    nap_child->argv[3] = strdup(binary);
  }
  nap_child->binary = strdup(nap_child->argv[3]);

  NaClXMutexUnlock(&nap->mu);

  /* TODO: fix dynamic text validation -jp */
  nap_child->skip_validator = 1;
  nap_child->main_exe_prevalidated = 1;
  
  /* calculate page addresses and sizes */
  dyncode_child = (void *)NaClUserToSys(nap_child, nap_child->dynamic_text_start);
  dyncode_size = NaClRoundPage(nap_child->dynamic_text_end - nap->dynamic_text_start);
  dyncode_npages = dyncode_size >> NACL_PAGESHIFT;
  tramp_size = NaClRoundPage(nap->static_text_end - NACL_SYSCALL_START_ADDR);
  tramp_npages = tramp_size >> NACL_PAGESHIFT;
  dyncode_pnum_child = NaClSysToUser(nap_child, (uintptr_t)dyncode_child) >> NACL_PAGESHIFT;
  parent_start_addr = nap->mem_start + NACL_SYSCALL_START_ADDR;
  child_start_addr = nap_child->mem_start + NACL_SYSCALL_START_ADDR;
  tramp_pnum = NaClSysToUser(nap, parent_start_addr) >> NACL_PAGESHIFT;

  /* map dynamic text into child */
  if (NaClMakeDynamicTextShared(nap_child) != LOAD_OK) {
    NaClLog(LOG_FATAL, "[cage id %d] failed to map dynamic text in NaClSysExecve()\n", nap_child->cage_id);
  }
  NaClVmmapAddWithOverwrite(&nap_child->mem_map,
                            dyncode_pnum_child,
                            dyncode_npages,
                            PROT_RX,
                            PROT_RX,
                            NACL_ABI_MAP_PRIVATE,
                            nap_child->text_shm,
                            0,
                            dyncode_size);

  /* add guard page mapping */
  NaClVmmapAdd(&nap_child->mem_map,
               0,
               NACL_SYSCALL_START_ADDR >> NACL_PAGESHIFT,
               NACL_ABI_PROT_NONE,
               NACL_ABI_PROT_NONE,
               NACL_ABI_MAP_PRIVATE,
               NULL,
               0,
               0);

  /*
   * The next pages up to NACL_TRAMPOLINE_END are the trampolines.
   * Immediately following that is the loaded text section.
   * These are collectively marked as PROT_READ | PROT_EXEC.
   */
  NaClLog(3,
          ("Trampoline/text region start 0x%08"NACL_PRIxPTR","
           " size 0x%08"NACL_PRIxS", end 0x%08"NACL_PRIxPTR"\n"),
          child_start_addr,
          tramp_size,
          child_start_addr + tramp_size);
  if (NaClMprotect((void *)child_start_addr, tramp_size, PROT_RW)) {
    NaClLog(LOG_FATAL, "NaClMemoryProtection: "
            "NaClMprotect(0x%08"NACL_PRIxPTR", "
            "0x%08"NACL_PRIxS", 0x%x) failed\n",
            child_start_addr,
            tramp_size,
            PROT_RW);
  }
  /* allocate and map child trampoline pages */
  NaClVmmapAddWithOverwrite(&nap_child->mem_map,
                            tramp_pnum,
                            tramp_npages,
                            PROT_RW,
                            PROT_RW,
                            MAP_ANON_PRIV,
                            NULL,
                            0,
                            0);
  if (!NaClPageAllocFlags((void **)&child_start_addr, tramp_size, NACL_ABI_MAP_ANON)) {
    NaClLog(LOG_FATAL, "%s\n", "child vmmap NaClPageAllocAtAddr failed!");
  }

  /* temporarily set RW page permissions for copy */
  NaClVmmapChangeProt(&nap_child->mem_map, tramp_pnum, tramp_npages, PROT_RW);
  NaClVmmapChangeProt(&nap->mem_map, tramp_pnum, tramp_npages, PROT_RW);
  if (NaClMprotect((void *)child_start_addr, tramp_size, PROT_RW) == -1) {
    NaClLog(LOG_FATAL, "%s\n", "child vmmap page NaClMprotect failed!");
  }
  if (NaClMprotect((void *)parent_start_addr, tramp_size, PROT_RW) == -1) {
    NaClLog(LOG_FATAL, "%s\n", "parent vmmap page NaClMprotect failed!");
  }

  /* setup trampolines */
  nap_child->nacl_syscall_addr = 0;
  NaClLog(2, "Initializing arch switcher\n");
  NaClInitSwitchToApp(nap_child);
  NaClLog(2, "Installing trampoline\n");
  NaClLoadTrampoline(nap_child);
  NaClLog(2, "Installing springboard\n");
  NaClLoadSpringboard(nap_child);
  /* copy the trampolines from parent */
  memmove((void *)child_start_addr, (void *)parent_start_addr, tramp_size);

  /*
   * NaClMemoryProtection also initializes the mem_map w/ information
   * about the memory pages and their current protection value.
   *
   * The contents of the dynamic text region will get remapped as
   * non-writable.
   */
  NaClLog(2, "Applying memory protection\n");
  if (NaClMemoryProtection(nap_child) != LOAD_OK) {
    NaClLog(LOG_FATAL, "%s\n", "child NaClMemoryProtection failed!");
  }

  /* reset permissions to executable */
  NaClLog(2, "Setting child permissions to executable\n");
  NaClVmmapChangeProt(&nap_child->mem_map, tramp_pnum, tramp_npages, PROT_RX);
  NaClLog(2, "Setting parent permissions to executable\n");
  NaClVmmapChangeProt(&nap->mem_map, tramp_pnum, tramp_npages, PROT_RX);

  NaClLog(2, "Child mprotect\n");
  if (NaClMprotect((void *)child_start_addr, tramp_size, PROT_RX) == -1) {
    NaClLog(LOG_FATAL, "%s\n", "child vmmap page NaClMprotect failed!");
  }
  NaClLog(2, "Parent mprotect\n");

  if (NaClMprotect((void *)parent_start_addr, tramp_size, PROT_RX) == -1) {
    NaClLog(LOG_FATAL, "%s\n", "parent vmmap page NaClMprotect failed!");
  }

  /* execute new binary, we pass NULL as parent natp since we're not basing the new thread off of this one. */
  ret = -NACL_ABI_ENOEXEC;
  NaClLog(1, "binary = %s\n", nap->binary);
  if (!NaClCreateThread(NULL, nap_child, child_argc, child_argv, nap_child->clean_environ)) {
    NaClLog(LOG_ERROR, "%s\n", "NaClCreateThread() failed");
    /* remove child cage */
    lind_exit(EXIT_FAILURE, child_cage_id);

    goto fail;
  }

  #ifdef TRACING
  endtime = gettimens();
  totaltime = endtime - starttime;
  NaClStraceExecve(nap->cage_id, sys_pathname, sys_argv_ptr, ret, totaltime);
  #endif

  /* wait for child to finish before cleaning up */
  NaClWaitForThreadToExit(nap_child);
  NaClReportExitStatus(nap, nap_child->exit_status);
  NaClAppThreadTeardown(natp);

  /* success */
  ret = 0;

fail:

  for (char **pp = new_argv; pp && *pp; pp++) {
    free(*pp);
  }
  free(new_argv);
  free(binary);

  #ifdef TRACING
  endtime = gettimens();
  totaltime = endtime - starttime;
  NaClStraceExecve(nap->cage_id, sys_pathname, sys_argv_ptr, ret, totaltime);
  #endif

  return ret;
}

#define WAIT_ANY (-1)
#define WAIT_ANY_PG 0

/*
 * Wait/Waitpid is used to reap a process's exited children, referred to as zombies
 * We use the NaClCheckZombies/NaClAddZombies/NaClRemoveZombies functions from sel_ldr.c to manage these zombies
 * Zombies are added to a parents zombie list when a child exits, in the NaClReportExitStatus function in sel_ldr_standard.c
 */
int32_t NaClSysWaitpid(struct NaClAppThread *natp,
                       int pid,
                       uint32_t *stat_loc,
                       int options) {

  /* seconds between thread switching */
  NACL_TIMESPEC_T const timeout = {1, 0};
  struct NaClApp *nap = natp->nap;
  struct NaClApp *nap_child = 0;
  uintptr_t sysaddr = NaClUserToSysAddrRangeProt(nap, (uintptr_t)stat_loc, 4, NACL_ABI_PROT_WRITE);
  int *stat_loc_ptr = sysaddr == kNaClBadAddress ? NULL : (int *)sysaddr;
  int pid_max = fork_num + 1;
  int ret = 0;
  struct NaClZombie* zombie;
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  NaClLog(1, "%s\n", "[NaClSysWaitpid] entered waitpid!");

  CHECK(nap->num_children < NACL_THREAD_MAX);
  if (stat_loc_ptr) *stat_loc_ptr = 0;

  NaClXMutexLock(&nap->children_mu);

  // First check if we have children, if not return ECHILD
  if (pid > pid_max || (!nap->num_children && !nap->zombies.num_entries)) {
    ret = -NACL_ABI_ECHILD;
    goto out;
  }

  // if WAIT_ANY, we'll busy loop on all children, except in the case of WNOHANG where we only loop once
  if (pid <= 0) {

    // check the zombies dynarray, and lazily return the first exited process if it exists
    zombie = NaClCheckZombies(nap);
    if (!zombie && (options & WNOHANG)) goto out; // exit here if WNOHANG, ret stays initialized to 0
    while(!zombie){

      /* make sure children exist, if not send ABI_ECHILD */
      if (!nap->num_children && !nap->zombies.num_entries) {
        ret = -NACL_ABI_ECHILD;
        goto out;
      }

      NaClLog(1, "Thread children count: %d\n", nap->num_children);
      // wait here until a signal is sent to check 
      NaClXCondVarTimedWaitRelative(&nap->children_cv, &nap->children_mu, &timeout);
      zombie = NaClCheckZombies(nap); //re-check on signal
    }

  } else {   // else we have an explicit waitpid with child pid given, lets wait for that pid
    int cage_id = pid;

    /* make sure children exist (check children and zombies) */
    nap_child = DynArrayGet(&nap->children, cage_id);
    zombie = NaClCheckZombieById(nap, cage_id);
    if (!nap_child && !zombie) {
      ret = -NACL_ABI_ECHILD;
      goto out;
    }
    
    if (!zombie && (options & WNOHANG)) goto out; // exit here if WNOHANG, ret stays initialized to 0

    NaClLog(1, "Thread children count: %d\n", nap->num_children);
    /* wait for child to finish */
    while (!zombie) {
      NaClXCondVarTimedWaitRelative(&nap->children_cv, &nap->children_mu, &timeout);
      zombie = NaClCheckZombieById(nap, cage_id);
    }
  }

  // return finished cage information
  if (stat_loc_ptr) *stat_loc_ptr = zombie->exit_status;
  ret = zombie->cage_id;
  NaClRemoveZombie(nap, zombie->cage_id);

out:
  NaClXMutexUnlock(&nap->children_mu);

  NaClLog(1, "[NaClSysWaitpid] pid = %d \n", pid);
  NaClLog(1, "[NaClSysWaitpid] status = %d \n", stat_loc_ptr ? *stat_loc_ptr : 0);
  NaClLog(1, "[NaClSysWaitpid] options = %d \n", options);
  NaClLog(1, "[NaClSysWaitpid] ret = %d \n", ret);
  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceWaitpid(nap->cage_id, pid, sysaddr, options, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysWait(struct NaClAppThread *natp, uint32_t *stat_loc) {
  int ret;

  NaClLog(1, "%s\n", "[NaClSysWait] entered wait! \n");
  ret = NaClSysWaitpid(natp, WAIT_ANY, stat_loc, 0);

  NaClLog(1, "[NaClSysWait] ret = %d \n", ret);
  return ret;
}

int32_t NaClSysWait4(struct NaClAppThread *natp, int pid, uint32_t *stat_loc, int options, void *rusage) {
  UNREFERENCED_PARAMETER(rusage);
  return NaClSysWaitpid(natp, pid, stat_loc, options);
}

int32_t NaClSysGethostname(struct NaClAppThread *natp, char *name, size_t len) {
  int32_t ret;
  uintptr_t sysaddr;
  struct NaClApp *nap = natp->nap;

  NaClLog(2, "Cage %d Entered NaClSysGethostname(0x%08"NACL_PRIxPTR", "
          "0x%08"NACL_PRIxPTR", "
          "%lu)\n",
          nap->cage_id, (uintptr_t) natp, (uintptr_t) name, len);
  
  sysaddr = NaClUserToSysAddrRangeProt(nap, (uintptr_t) name, len, NACL_ABI_PROT_WRITE);
  if (kNaClBadAddress == sysaddr) {
    NaClLog(2, "NaClSysGethostname could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    ret = -NACL_ABI_EFAULT;
    return ret;
  }
  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  
  ret = lind_gethostname ((void *) sysaddr, len, nap->cage_id);
  
  NaClLog(2, "NaClSysGethostname: returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceGethostname(nap->cage_id, sysaddr, len, ret, totaltime);
  #endif
  
  return ret;
}

int32_t NaClSysGetifaddrs(struct NaClAppThread *natp, char *buf, size_t len) {
  int32_t ret;
  uintptr_t sysaddr;
  struct NaClApp *nap = natp->nap;

  NaClLog(2,"Cage %d Entered NaClSysGetifaddrs(0x%08"NACL_PRIxPTR", "
          "0x%08"NACL_PRIxPTR", "
          "%lu)\n",
          nap->cage_id, (uintptr_t) natp, (uintptr_t) buf, len);
  
  sysaddr = NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, len, NACL_ABI_PROT_WRITE);
  if (kNaClBadAddress == sysaddr) {
    NaClLog(2, "NaClSysGetifaddrs could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    ret = -NACL_ABI_EFAULT;
    return ret;
  }
  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  
  ret = lind_getifaddrs ((char *) sysaddr, len, nap->cage_id);
  
  NaClLog(2, "NaClSysGetifaddrs: returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceGetifaddrs(nap->cage_id, sysaddr, len, ret, totaltime);
  #endif
  
  return ret;
}

int32_t NaClSysSocket(struct NaClAppThread *natp, int domain, int type, int protocol) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  NaClLog(2, "Cage %d Entered NaClSysSocket(0x%08"NACL_PRIxPTR", "
          "%d, %d, %d)\n",
          nap->cage_id, (uintptr_t) natp, domain, type, protocol);
  #ifdef TRACING
  long long starttime = gettimens();
  #endif        
  
  ret = lind_socket (domain, type, protocol, nap->cage_id);

  NaClLog(2, "NaClSysSocket: returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceSocket(nap->cage_id, domain, type, protocol, ret, totaltime);
  #endif
  
  return ret;
}

int32_t NaClSysSend(struct NaClAppThread *natp, int sockfd, size_t len, int flags, const void *buf) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;
  const void *sysbufaddr = (const void*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, len, NACL_ABI_PROT_READ);
  NaClLog(2, "Cage %d Entered NaClSysSend(0x%08"NACL_PRIxPTR", "
          "%d, %ld, %d, 0x%08"NACL_PRIxPTR")\n",
          nap->cage_id, (uintptr_t) natp, sockfd, len, flags, (uintptr_t) buf);

  if ((void*) kNaClBadAddress == sysbufaddr) {
    NaClLog(2, "NaClSysSend could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_send(sockfd, sysbufaddr, len, flags, nap->cage_id);
  NaClLog(2, "NaClSysSend: returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceSend(nap->cage_id, sockfd, len, flags, sysbufaddr, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysSendto(struct NaClAppThread *natp, int sockfd, const void *buf, size_t len,
                         int flags, const struct sockaddr *dest_addr, socklen_t addrlen) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  const void *sysbufaddr = (const void*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, len, NACL_ABI_PROT_READ);
  const void *syssockaddraddr = dest_addr == NULL ? NULL : (const void*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) dest_addr, addrlen, NACL_ABI_PROT_READ);
  NaClLog(2, "Cage %d Entered NaClSysSendto(0x%08"NACL_PRIxPTR", "
          "%d, 0x%08"NACL_PRIxPTR", %ld, %d, 0x%08"NACL_PRIxPTR", %d)\n",
          nap->cage_id, (uintptr_t) natp, sockfd, (uintptr_t) buf, len, flags, (uintptr_t) dest_addr, addrlen);

  if ((void*) kNaClBadAddress == sysbufaddr) {
    NaClLog(2, "NaClSysSendto could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }
  if ((void*) kNaClBadAddress == syssockaddraddr) {
    NaClLog(2, "NaClSysSendto could not translate sockaddr address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_sendto(sockfd, sysbufaddr, len, flags, syssockaddraddr, addrlen, nap->cage_id);
  NaClLog(2, "NaClSysSendto: returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceSendto(nap->cage_id, sockfd, sysbufaddr, len, flags, (uintptr_t)syssockaddraddr, addrlen, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysRecv(struct NaClAppThread *natp, int sockfd, size_t len, int flags, void *buf) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  void *sysbufaddr = (void*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, len, NACL_ABI_PROT_WRITE);
  NaClLog(2, "Cage %d Entered NaClSysRecv(0x%08"NACL_PRIxPTR", "
          "%d, %ld, %d, 0x%08"NACL_PRIxPTR")\n",
          nap->cage_id, (uintptr_t) natp, sockfd, len, flags, (uintptr_t) buf);

  if ((void*) kNaClBadAddress == sysbufaddr) {
    NaClLog(2, "NaClSysRecv could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_recv(sockfd, sysbufaddr, len, flags, nap->cage_id);
  NaClLog(2, "NaClSysRecv: returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceRecv(nap->cage_id, sockfd, sysbufaddr, len, flags, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysRecvfrom(struct NaClAppThread *natp, int sockfd, void *buf, size_t len, int flags,
                           struct sockaddr *src_addr, socklen_t *addrlen) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  void *sysbufaddr = (void*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, len, NACL_ABI_PROT_WRITE);
  socklen_t *sysaddrlenaddr = addrlen == NULL ? NULL : (void*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) addrlen, sizeof(socklen_t), NACL_ABI_PROT_WRITE);
  struct sockaddr *sysaddraddr;

  NaClLog(2, "Cage %d Entered NaClSysRecvfrom(0x%08"NACL_PRIxPTR", "
          "%d, %p"NACL_PRIxPTR", %ld, %d, 0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR")\n",
          nap->cage_id, (uintptr_t) natp, sockfd, buf, len, flags, (uintptr_t)src_addr, (uintptr_t)addrlen);

  if ((void*) kNaClBadAddress == sysbufaddr) {
    NaClLog(2, "NaClSysRecvfrom could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }
  if ((void*) kNaClBadAddress == sysaddrlenaddr) {
    NaClLog(2, "NaClSysRecvfrom could not translate address length pointer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  if(sysaddrlenaddr != NULL) {
    sysaddraddr = (struct sockaddr*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) src_addr, sizeof(struct sockaddr_in6), NACL_ABI_PROT_WRITE);//we use sockaddr_in6 to be conservative
 
    if ((void*) kNaClBadAddress == sysaddraddr) {
      NaClLog(2, "NaClSysRecvfrom could not translate socket address address, returning %d\n", -NACL_ABI_EFAULT);
      return -NACL_ABI_EFAULT;
    }
  } else {
    if(src_addr != NULL) {
      NaClLog(2, "NaClSysRecvfrom had a 0 length specified but the address was not NULL, returning %d\n", -NACL_ABI_EINVAL);
      return -NACL_ABI_EINVAL;
    } else {
      sysaddraddr = NULL;
    }
  }
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_recvfrom(sockfd, sysbufaddr, len, flags, sysaddraddr, sysaddrlenaddr, nap->cage_id);
  NaClLog(2, "NaClSysRecvfrom: returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceRecvfrom(nap->cage_id, sockfd, sysbufaddr, len, flags, (uintptr_t)sysaddraddr, sysaddrlenaddr, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysShutdown(struct NaClAppThread *natp, int sockfd, int how)
{
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  NaClLog(2, "Cage %d Entered NaClSysShutdown(0x%08"NACL_PRIxPTR", %d, %d)\n",
          nap->cage_id, (uintptr_t) natp, sockfd, how);
  #ifdef TRACING
  long long starttime = gettimens();
  #endif        
  
  ret = lind_shutdown(sockfd, how, nap->cage_id);
  NaClLog(2, "NaClSysShutdown returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceShutdown(nap->cage_id, sockfd, how, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysGetuid(struct NaClAppThread *natp)
{
  struct NaClApp *nap = natp->nap;
  
  #ifdef TRACING
  long long starttime = gettimens();
  #endif
  int ret = lind_getuid(nap->cage_id);
  NaClLog(2, "NaClSysGetuid returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceGetuid(nap->cage_id, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysGeteuid(struct NaClAppThread *natp)
{
  struct NaClApp *nap = natp->nap;
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  int ret = lind_geteuid(nap->cage_id);
  NaClLog(2, "NaClSysGeteuid returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceGeteuid(nap->cage_id, ret,totaltime);
  #endif

  return ret;
}

int32_t NaClSysGetgid(struct NaClAppThread *natp)
{
  struct NaClApp *nap = natp->nap;
  int ret;

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_getgid(nap->cage_id);
  NaClLog(2, "NaClSysGetgid returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceGetgid(nap->cage_id, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysGetegid(struct NaClAppThread *natp)
{
  struct NaClApp *nap = natp->nap;
  int ret;

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_getegid(nap->cage_id);
  NaClLog(2, "NaClSysGetegid returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceGetegid(nap->cage_id, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysFlock(struct NaClAppThread *natp, int fd, int operation)
{
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  NaClLog(2, "Cage %d Entered NaClSysFlock(0x%08"NACL_PRIxPTR", %d, %d)\n",
          nap->cage_id, (uintptr_t) natp, fd, operation);
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_flock(fd, operation, nap->cage_id);
  NaClLog(2, "NaClSysFlock returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceFlock(nap->cage_id, fd, operation, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysGetsockopt(struct NaClAppThread *natp, int sockfd, int level, int optname, void *optval, socklen_t *optlen) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;
  unsigned int *syslenaddr = (unsigned int*) NaClUserToSysAddrProt(nap, (uintptr_t) optlen, NACL_ABI_PROT_WRITE);
  void *sysvaladdr;

  if ((void*) kNaClBadAddress == syslenaddr) {
    NaClLog(2, "NaClSysGetsockopt could not translate optlen address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  sysvaladdr = (void*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) optval, *syslenaddr, NACL_ABI_PROT_WRITE);
  NaClLog(2, "Cage %d Entered NaClSysGetsockopt(0x%08"NACL_PRIxPTR", %d, %d, %d, 0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR")\n",
          nap->cage_id, (uintptr_t) natp, sockfd, level, optname, (uintptr_t) optval, (uintptr_t) optlen);

  if ((void *) kNaClBadAddress == sysvaladdr) {
    NaClLog(2, "NaClSysGetsockopt could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }
  
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_getsockopt(sockfd, level, optname, sysvaladdr, syslenaddr, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceGetsockopt(nap->cage_id, sockfd, level, optname, sysvaladdr, syslenaddr, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysSetsockopt(struct NaClAppThread *natp, int sockfd, int level, int optname, const void *optval, socklen_t optlen) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;
  
  const void *sysvaladdr = (const void*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) optval, optlen, NACL_ABI_PROT_READ);
  NaClLog(2, "Cage %d Entered NaClSysSetsockopt(0x%08"NACL_PRIxPTR", %d, %d, %d, 0x%08"NACL_PRIxPTR", %u)\n",
          nap->cage_id, (uintptr_t) natp, sockfd, level, optname, (uintptr_t) optval, optlen);

  if ((void*) kNaClBadAddress == sysvaladdr) {
    NaClLog(2, "NaClSysSetsockopt could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_setsockopt(sockfd, level, optname, sysvaladdr, optlen, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceSetsockopt(nap->cage_id, sockfd, level, optname, sysvaladdr, optlen, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysFstatfs(struct NaClAppThread *natp,
                       int                  d,
                       struct lind_statfs   *buf) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  struct lind_statfs *sysbufaddr = (struct lind_statfs*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, sizeof(struct lind_statfs), NACL_ABI_PROT_WRITE);
  NaClLog(2, "Cage %d Entered NaClSysFstatfs(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR")\n",
          nap->cage_id, (uintptr_t) natp, d, (uintptr_t) buf);

  if ((void*) kNaClBadAddress == sysbufaddr) {
    NaClLog(2, "NaClSysFstatfs could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_fstatfs(d, (struct statfs *) sysbufaddr, nap->cage_id);

  if(ret > 0) ret = 0;

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceFstatfs(nap->cage_id, d, (uintptr_t) sysbufaddr, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysStatfs(struct NaClAppThread *natp,
                      const char           *pathname,
                      struct lind_statfs   *buf) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;
  struct lind_statfs *sysbufaddr = (struct lind_statfs*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) buf, sizeof(struct lind_statfs), NACL_ABI_PROT_WRITE);
  char           path[NACL_CONFIG_PATH_MAX];

  ret = CopyPathFromUser(nap, path, sizeof(path), (uintptr_t) pathname);

  NaClLog(2, "Cage %d Entered NaClSysStatfs(0x%08"NACL_PRIxPTR", %s, 0x%08"NACL_PRIxPTR")\n",
          nap->cage_id, (uintptr_t) natp, path, (uintptr_t) buf);

  if (ret) {
    NaClLog(2, "NaClSysStatfs could not translate path address, returning %d\n", ret);
    return ret;
  }

  if ((void*) kNaClBadAddress == sysbufaddr) {
    NaClLog(2, "NaClSysStatfs could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_statfs(path, (struct statfs *) sysbufaddr, nap->cage_id);

  if(ret > 0) ret = 0;

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceStatfs(nap->cage_id, path, (uintptr_t) sysbufaddr, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysGetsockname(struct NaClAppThread *natp, 
                        int sockfd, 
                        struct sockaddr * addr,
                        socklen_t * addrlen) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;
  struct sockaddr * sysaddr = (struct sockaddr*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) addr, sizeof(struct sockaddr), NACL_ABI_PROT_WRITE);
  socklen_t * sysaddrlen = (socklen_t*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) addrlen, sizeof(socklen_t), NACL_ABI_PROT_WRITE);

  if ((void*) kNaClBadAddress == sysaddr) {
    NaClLog(2, "NaClSysGetpeername could not translate socket address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  if ((void*) kNaClBadAddress == sysaddrlen) {
    NaClLog(2, "NaClSysGetpeername could not translate addrlen address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  NaClLog(2, "Cage %d Entered NaClSysGetsockname(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR")\n", 
          nap->cage_id, (uintptr_t) natp, sockfd, (uintptr_t) addr, (uintptr_t) addrlen);
  
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_getsockname(sockfd, sysaddr, sysaddrlen, nap->cage_id);
  NaClLog(2, "NaClSysGetsockname returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceGetsockname(nap->cage_id, sockfd, (uintptr_t) sysaddr, sysaddrlen, ret, totaltime);
  #endif
  
  return ret; 
}

int32_t NaClSysGetpeername(struct NaClAppThread *natp, 
                        int sockfd, 
                        struct sockaddr * addr,
                        socklen_t * addrlen) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  struct sockaddr * sysaddr = (struct sockaddr*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) addr, sizeof(struct sockaddr), NACL_ABI_PROT_WRITE);
  socklen_t * sysaddrlen = (socklen_t*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) addrlen, sizeof(socklen_t), NACL_ABI_PROT_WRITE);

  if ((void*) kNaClBadAddress == sysaddr) {
    NaClLog(2, "NaClSysGetpeername could not translate socket address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  if ((void*) kNaClBadAddress == sysaddrlen) {
    NaClLog(2, "NaClSysGetpeername could not translate addrlen address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  NaClLog(2, "Cage %d Entered NaClSysGetpeername(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR")\n", 
          nap->cage_id, (uintptr_t) natp, sockfd, (uintptr_t) addr, (uintptr_t) addrlen);
  
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_getpeername(sockfd, sysaddr, sysaddrlen, nap->cage_id);
  NaClLog(2, "NaClSysGetpeername returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceGetpeername(nap->cage_id, sockfd, (uintptr_t)sysaddr, sysaddrlen, ret, totaltime);
  #endif

  return ret; 
}

int32_t NaClSysAccess(struct NaClAppThread *natp,
                      const char *file, int mode) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;
  char path[NACL_CONFIG_PATH_MAX];

  ret = CopyPathFromUser(nap, path, sizeof(path), (uintptr_t) file);

  NaClLog(2, "Cage %d Entered NaClSysAccess(0x%08"NACL_PRIxPTR", %s, %d)\n",
          nap->cage_id, (uintptr_t) natp, path, mode);

  if (ret) {
    NaClLog(2, "NaClSysAccess could not translate path address, returning %d\n", ret);
    return ret;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_access(path, mode, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceAccess(nap->cage_id, path, mode, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysTruncate(struct NaClAppThread *natp,
                        uint32_t file, int length) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;
  char path[NACL_CONFIG_PATH_MAX];

  ret = CopyPathFromUser(nap, path, sizeof(path), (uintptr_t) file);

  NaClLog(2, "Cage %d Entered NaClSysTruncate(0x%08"NACL_PRIxPTR", %s, %d)\n",
          nap->cage_id, (uintptr_t) natp, path, length);

  if (ret) {
    NaClLog(2, "NaClSysTruncate could not translate path address, returning %d\n", ret);
    return ret;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_truncate(path, length, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceTruncate(nap->cage_id, path, length, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysFtruncate(struct NaClAppThread *natp,
                         int fd, int length) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  NaClLog(2, "Cage %d Entered NaClSysFTruncate(0x%08"NACL_PRIxPTR", %d, %d)\n",
          nap->cage_id, (uintptr_t) natp, fd, length);

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_ftruncate(fd, length, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceFtruncate(nap->cage_id, fd, length, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysConnect(struct NaClAppThread *natp,
                       int sockfd,
                       const struct sockaddr *addr, 
                       socklen_t addrlen) {
  struct NaClApp *nap = natp->nap;
  const struct sockaddr* sysvaladdr;
  int32_t ret;

  NaClLog(2, "Cage %d Entered NaClSysConnect(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR", %d)\n",
          nap->cage_id, (uintptr_t) natp, sockfd, (uintptr_t) addr, addrlen);
  
  sysvaladdr = (struct sockaddr*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) addr, addrlen, NACL_ABI_PROT_READ);

  if ((void*) kNaClBadAddress == sysvaladdr) {
    NaClLog(2, "NaClSysConnect could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_connect(sockfd, sysvaladdr, addrlen, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceConnect(nap->cage_id, sockfd, (uintptr_t)sysvaladdr, addrlen, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysAccept(struct NaClAppThread *natp,
                      int sockfd, 
                      struct sockaddr *addr, 
                      socklen_t *addrlen) {
  struct NaClApp *nap = natp->nap;
  struct sockaddr* sysvaladdr;
  socklen_t* syslenaddr;
  int32_t ret;

  NaClLog(2, "Cage %d Entered NaClSysAccept(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR")\n",
          nap->cage_id, (uintptr_t) natp, sockfd, (uintptr_t) addr, (uintptr_t) addrlen);

  syslenaddr = addrlen == NULL ? NULL : (socklen_t*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) addrlen, sizeof(socklen_t), NACL_ABI_PROT_WRITE);
 
  if ((void*) kNaClBadAddress == syslenaddr) {
    NaClLog(2, "NaClSysAccept could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT; // As we return userfd instead of a retvalue, changed ret with userfd.
  }

  if(syslenaddr != NULL) {
    sysvaladdr = (struct sockaddr*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) addr, sizeof(struct sockaddr_in6), NACL_ABI_PROT_WRITE);
 
    if ((void*) kNaClBadAddress == sysvaladdr) {
      NaClLog(2, "NaClSysAccept could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
      return -NACL_ABI_EFAULT; // As we return userfd instead of a retvalue, changed ret with userfd.
    }
  } else {
    if(addr != NULL) {
      NaClLog(2, "NaClSysAccept had a 0 length specified but the address was not NULL, returning %d\n", -NACL_ABI_EINVAL);
      return -NACL_ABI_EINVAL;
    } else {
      sysvaladdr = NULL;
    }
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_accept(sockfd, sysvaladdr, syslenaddr, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceAccept(nap->cage_id, sockfd, (uintptr_t)sysvaladdr, syslenaddr, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysBind(struct NaClAppThread *natp,
                       int sockfd, 
                       const struct sockaddr *addr,
                       socklen_t addrlen) { 
  
  struct NaClApp *nap = natp->nap;
  const struct sockaddr* sysvaladdr;
  int32_t ret;

  NaClLog(2, "Cage %d Entered NaClSysBind(0x%08"NACL_PRIxPTR", %d, %d, 0x%08"NACL_PRIxPTR")\n",
          nap->cage_id, (uintptr_t) natp, sockfd, addrlen, (uintptr_t) addr);

  sysvaladdr = (struct sockaddr*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) addr, addrlen, NACL_ABI_PROT_READ);

  if ((void*) kNaClBadAddress == sysvaladdr) {
    NaClLog(2, "NaClSysBind could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_bind(sockfd, sysvaladdr, addrlen, nap->cage_id);
  NaClLog(2, "NaClSysBind returning %d\n", ret);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceBind(nap->cage_id, sockfd, (uintptr_t) sysvaladdr, addrlen, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysListen(struct NaClAppThread *natp,
                       int sockfd, 
                       int backlog) {
  
  struct NaClApp *nap = natp->nap;
  int32_t ret;

  NaClLog(2, "Cage %d Entered NaClSysListen(0x%08"NACL_PRIxPTR", %d, %d)\n",
          nap->cage_id, (uintptr_t) natp, sockfd, backlog);
  
  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_listen(sockfd, backlog, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceListen(nap->cage_id, sockfd, backlog, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysFcntlGet(struct NaClAppThread *natp, int fd, int cmd) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  NaClLog(2, "Cage %d Entered NaClSysFcntlGet(0x%08"NACL_PRIxPTR", %d, %d)\n",
          nap->cage_id, (uintptr_t) natp, fd, cmd);

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_fcntl_get(fd, cmd, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceFcntlGet(nap->cage_id, fd, cmd, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysFcntlSet(struct NaClAppThread *natp, int fd, int cmd, long set_op) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;

  NaClLog(2, "Cage %d Entered NaClSysFcntlSet(0x%08"NACL_PRIxPTR", %d, %d, %ld)\n",
          nap->cage_id, (uintptr_t) natp, fd, cmd, set_op);

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_fcntl_set(fd, cmd, set_op, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceFcntlSet(nap->cage_id, fd, cmd, set_op, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysPoll(struct NaClAppThread *natp, struct pollfd *fds, nfds_t nfds, int timeout) {
  struct NaClApp *nap = natp->nap;
  
  int retval = 0;
  struct pollfd *fds_sysaddr;

  fds_sysaddr = (struct pollfd *) NaClUserToSysAddrRangeProt(nap, (uintptr_t) fds, nfds * sizeof(struct pollfd), NACL_ABI_PROT_WRITE);

  if ((void*) kNaClBadAddress == fds_sysaddr) {
    NaClLog(2, "NaClSysPoll could not translate fds array, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_poll(fds_sysaddr, nfds, timeout, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStracePoll(nap->cage_id, (uintptr_t)fds_sysaddr, nfds, timeout, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysEpollCreate(struct NaClAppThread *natp, int size) {
  struct NaClApp *nap = natp->nap;
  int32_t ret;

  NaClLog(2, "Cage %d Entered NaClSysEpollCreate(0x%08"NACL_PRIxPTR", %d)\n",
          nap->cage_id, (uintptr_t) natp, size);

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_epoll_create(size, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceEpollCreate(nap->cage_id, size, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysEpollCtl(struct NaClAppThread *natp, int epfd, int op, int fd, struct epoll_event *event) {
  struct NaClApp *nap = natp->nap;
  struct epoll_event *eventsysaddr;
  int32_t ret;

  NaClLog(2, "Cage %d Entered NaClSysEpollCtl(0x%08"NACL_PRIxPTR", %d, %d, %d, 0x%08"NACL_PRIxPTR")\n",
          nap->cage_id, (uintptr_t) natp, epfd, op, fd, (uintptr_t) event);

  eventsysaddr = (struct epoll_event*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) event, sizeof(eventsysaddr), NACL_ABI_PROT_READ);

  if ((void*) kNaClBadAddress == eventsysaddr) {
    NaClLog(2, "NaClSysEpollCtl could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  ret = lind_epoll_ctl(epfd, op, fd, eventsysaddr, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceEpollCtl(nap->cage_id, epfd, op, fd, (uintptr_t) eventsysaddr, ret, totaltime);
  #endif

  return ret;
}

int32_t NaClSysEpollWait(struct NaClAppThread *natp, int epfd, struct epoll_event *events, int maxevents, int timeout) {
  struct NaClApp *nap = natp->nap;
  struct epoll_event *eventsysaddr;
  int retval = 0;
  struct epoll_event *sys_event_array;

  NaClLog(2, "Cage %d Entered NaClSysEpollWait(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR", %d, %d)\n",
          nap->cage_id, (uintptr_t) natp, epfd, (uintptr_t) events, maxevents, timeout);

  sys_event_array = (struct epoll_event*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) events, sizeof(eventsysaddr), NACL_ABI_PROT_WRITE);

  if ((void*) kNaClBadAddress == sys_event_array) {
    NaClLog(2, "NaClSysEpollCtl could not translate buffer address, returning %d\n", -NACL_ABI_EFAULT);
    return -NACL_ABI_EFAULT;
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_epoll_wait(epfd, sys_event_array, maxevents, timeout, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceEpollWait(nap->cage_id, epfd, (uintptr_t) sys_event_array, maxevents, timeout, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysSelect(struct NaClAppThread *natp, int nfds, fd_set *readfds, 
                      fd_set *writefds, fd_set *exceptfds, struct timeval *timeout) {
  struct NaClApp *nap = natp->nap;
  int retval;
  fd_set *syswritefds = NULL;
  fd_set *sysreadfds = NULL;
  fd_set *sysexceptfds = NULL;
  struct timeval *nacltimeout = NULL;

  NaClLog(2, "Cage %d Entered NaClSysSelect(0x%08"NACL_PRIxPTR", %d, 0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR", 0x%08"NACL_PRIxPTR")\n",
          nap->cage_id, (uintptr_t)natp, nfds, (uintptr_t)readfds, (uintptr_t)writefds, (uintptr_t)exceptfds, (uintptr_t)timeout);

  if (readfds) {
    sysreadfds = (fd_set*)NaClUserToSysAddrRangeProt(nap, (uintptr_t)readfds, sizeof(fd_set), NACL_ABI_PROT_WRITE);

    if ((void*)kNaClBadAddress == sysreadfds) {
      NaClLog(2, "NaClSysSelect could not translate read fds address, returning %d\n", -NACL_ABI_EFAULT);
      return -NACL_ABI_EFAULT;
    }
  }

  if (writefds) {
    syswritefds = (fd_set*)NaClUserToSysAddrRangeProt(nap, (uintptr_t)writefds, sizeof(fd_set), NACL_ABI_PROT_WRITE);

    if ((void*)kNaClBadAddress == syswritefds) {
      NaClLog(2, "NaClSysSelect could not translate write fds address, returning %d\n", -NACL_ABI_EFAULT);
      return -NACL_ABI_EFAULT;
    }
  }

  if (exceptfds) {
    sysexceptfds = (fd_set*)NaClUserToSysAddrRangeProt(nap, (uintptr_t)exceptfds, sizeof(fd_set), NACL_ABI_PROT_WRITE);

    if ((void*)kNaClBadAddress == sysexceptfds) {
      NaClLog(2, "NaClSysSelect could not translate except fds address, returning %d\n", -NACL_ABI_EFAULT);
      return -NACL_ABI_EFAULT;
    }
  }

  if (timeout) {
    nacltimeout = (struct timeval*)NaClUserToSysAddrRangeProt(nap, (uintptr_t)timeout, sizeof(struct timeval), NACL_ABI_PROT_READ);

    if ((void*)kNaClBadAddress == nacltimeout) {
      NaClLog(2, "NaClSysSelect could not translate timeout address, returning %d\n", -NACL_ABI_EFAULT);
      return -NACL_ABI_EFAULT;
    }
  }

  #ifdef TRACING
  long long starttime = gettimens();
  #endif

  retval = lind_select(nfds, sysreadfds, syswritefds, sysexceptfds, nacltimeout, nap->cage_id);

  #ifdef TRACING
  long long endtime = gettimens();
  long long totaltime = endtime - starttime;
  NaClStraceSelect(nap->cage_id, nfds, (uintptr_t)sysreadfds, (uintptr_t)syswritefds, (uintptr_t)sysexceptfds, (uintptr_t)nacltimeout, retval, totaltime);
  #endif

  return retval;
}

int32_t NaClSysSigaction(struct NaClAppThread *natp, int32_t sig, const struct nacl_abi_sigaction *act, struct nacl_abi_sigaction *oact) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;
  const struct nacl_abi_sigaction *sysact = NULL;
  struct nacl_abi_sigaction *sysoact = NULL;

  if (act) {
    sysact = (const struct nacl_abi_sigaction*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) act, sizeof(struct nacl_abi_sigaction), NACL_ABI_PROT_READ);

    if ((void*) kNaClBadAddress == sysact) {
        NaClLog(2, "NaClSysSigaction could not translate act, returning %d\n", -NACL_ABI_EFAULT);
        return -NACL_ABI_EFAULT;
    }
  }

  if (oact) {
    sysoact = (struct nacl_abi_sigaction*) NaClUserToSysAddrRangeProt(nap, (uintptr_t) oact, sizeof(struct nacl_abi_sigaction), NACL_ABI_PROT_WRITE);

    if ((void*) kNaClBadAddress == sysoact) {
        NaClLog(2, "NaClSysSigaction could not translate oact, returning %d\n", -NACL_ABI_EFAULT);
        return -NACL_ABI_EFAULT;
    }
  }

  ret = lind_sigaction(sig, sysact, sysoact, nap->cage_id);
  return ret;
}

int32_t NaClSysKill(struct NaClAppThread *natp, int32_t targetcageid, int32_t sig) {
  struct NaClApp *nap = natp->nap;
  return lind_kill(targetcageid, sig, nap->cage_id);
}

int32_t NaClSysSigprocmask(struct NaClAppThread *natp, int32_t how, const uint64_t *nacl_set, uint64_t *nacl_oldset) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;
  const uint64_t *sysset = NULL;
  uint64_t *sysoldset = NULL;

  if (nacl_set) {
    sysset = (const uint64_t *) NaClUserToSysAddrRangeProt(nap, (uintptr_t) nacl_set, sizeof(uint64_t), NACL_ABI_PROT_READ);

    if ((void *) kNaClBadAddress == sysset) {
        NaClLog(2, "NaClSysSigprocmask could not translate set, returning %d\n", -NACL_ABI_EFAULT);
        return -NACL_ABI_EFAULT;
    }
  }

  if (nacl_oldset) {
    sysoldset = (uint64_t *) NaClUserToSysAddrRangeProt(nap, (uintptr_t) nacl_oldset, sizeof(uint64_t), NACL_ABI_PROT_READ);

    if ((void *) kNaClBadAddress == sysoldset) {
        NaClLog(2, "NaClSysSigprocmask could not translate oldset, returning %d\n", -NACL_ABI_EFAULT);
        return -NACL_ABI_EFAULT;
    }
  }

  ret = lind_sigprocmask(how, sysset, sysoldset, nap->cage_id);
  return ret;
}

int32_t NaClSysLindsetitimer(struct NaClAppThread *natp, int32_t which, const struct itimerval *new_value, struct itimerval *old_value) {
  int32_t ret;
  struct NaClApp *nap = natp->nap;
  const struct itimerval *sys_new_value = NULL;
  struct itimerval *sys_old_value = NULL;

  if (new_value) {
    sys_new_value = (const struct itimerval *) NaClUserToSysAddrRangeProt(nap, (uintptr_t) new_value, sizeof(struct itimerval), NACL_ABI_PROT_READ);

    if ((void *) kNaClBadAddress == sys_new_value) {
      NaClLog(2, "NaclSysLindsetitimer could not translate new_value, returning %d\n", -NACL_ABI_EFAULT);
      return -NACL_ABI_EFAULT;
    }
  }

  if (old_value) {
    sys_old_value = (struct itimerval *) NaClUserToSysAddrRangeProt(nap, (uintptr_t) old_value, sizeof(struct itimerval), NACL_ABI_PROT_WRITE);

    if ((void *) kNaClBadAddress == sys_old_value) {
      NaClLog(2, "NaclSysLindsetitimer could not translate old_value, returning %d\n", -NACL_ABI_EFAULT);
      return -NACL_ABI_EFAULT;
    }
  }

  ret = lind_lindsetitimer(which, sys_new_value, sys_old_value, nap->cage_id);
  return ret;
}

/*
 * For what happens on return from an untrusted signal handler see the comment above the function
 * NaClTrampolineRegRestore in arch/x86_64/nacl_syscall_64.S
 */
int32_t NaClSysSigmaskSigreturn(struct NaClAppThread *natp) {
  sigset_t s;
  sigemptyset(&s);
  natp->exception_flag = 0;
  pthread_sigmask(SIG_SETMASK, &s, NULL);
  return 0;
}