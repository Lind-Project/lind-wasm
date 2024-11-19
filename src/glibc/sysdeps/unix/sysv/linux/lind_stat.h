/*
 * lind_stat.h
 *
 *  Created on: Jul 24, 2013
 *      Author: sji
 */

#ifndef LIND_STAT_H_
#define LIND_STAT_H_

#include <stdint.h>

struct lind_timespec {
    uint64_t tv_sec;
    uint64_t tv_nsec;
};

struct lind_stat {
    uint64_t st_dev;     /* Device.  */
    uint64_t st_ino;     /* File serial number.  */
    uint32_t st_mode;       /* File mode.  */
    uint32_t st_nlink;     /* Link count.  */
    uint32_t st_uid;     /* User ID of the file's owner. */
    uint32_t st_gid;     /* Group ID of the file's group.*/
    uint64_t st_rdev;        /* Device number, if device.  */
    int64_t st_size;            /* Size of file, in bytes.  */
    int32_t st_blksize; /* Optimal block size for I/O.  */
    uint32_t st_blocks;       /* Number 512-byte blocks allocated. */
    /* Nanosecond resolution timestamps are stored in a format
       equivalent to 'struct timespec'.  This is the type used
       whenever possible but the Unix namespace rules do not allow the
       identifier 'timespec' to appear in the <sys/stat.h> header.
       Therefore we have to handle the use of this header in strictly
       standard-compliant sources special.  */
    struct lind_timespec st_atim;        /* Time of last access.  */
    struct lind_timespec st_mtim;        /* Time of last modification.  */
    struct lind_timespec st_ctim;        /* Time of last status change.  */
};

struct lind_statfs {
    int64_t f_type;
    int64_t f_bsize;
    uint64_t f_blocks;
    uint64_t f_bfree;
    uint64_t f_bavail;
    uint64_t f_files;
    uint64_t f_ffree;
    struct { int32_t __val[2]; } f_fsid;
    int64_t f_namelen;
    int64_t f_frsize;
    int64_t f_flags;
    int64_t f_spare[4];
};

struct lind_ipc_perm {  /* must be renamed when ABI is exported */
  int32_t               shm_key;       
  uint32_t              shm_uid;
  uint32_t              shm_gid;
  uint32_t              shm_cuid;
  uint32_t              shm_cgid;
  uint16_t              shm_mode;
  uint16_t              shm_pad1;
  uint16_t              shm_seq;
  uint16_t              shm_pad2;
  uint32_t              shm_unused1;
  uint32_t              shm_unused2;
};

struct lind_shmid_ds {  /* must be renamed when ABI is exported */
  struct lind_ipc_perm      shm_perm;
  uint32_t                  shm_segsz;
  int64_t                   shm_atime;
  int64_t                   shm_dtime;
  int64_t                   shm_ctime;
  uint32_t                  shm_cpid;
  uint32_t                  shm_lpid;
  uint32_t                  shm_nattch;
};


#endif /* LIND_STAT_H_ */
