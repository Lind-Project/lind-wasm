/*
 * lind_platform.h
 *
 *  Created on: Jul 23, 2013
 *      Author: sji
 *  Recreated on: Sep 24, 2021
 *      Author: jesings
 */

#ifndef LIND_PLATFORM_H_
#define LIND_PLATFORM_H_

#include <sys/types.h>
#if NACL_LINUX
# include <sys/statfs.h>
#endif
#include "lind_stat.h"
#include <sys/socket.h>
#include <sys/select.h>
#include <sys/poll.h>
#include <sys/epoll.h>
#include <sys/shm.h>
#include <sys/time.h>
#include <signal.h>

/* avoid errors caused by conflicts with feature_test_macros(7) */
#undef _POSIX_C_SOURCE
#undef _XOPEN_SOURCE

#include <unistd.h>
#include <stdint.h>
#include <stdbool.h>

#if NACL_OSX
# define __SOCKADDR_ARG struct sockaddr
#endif

#define LIND_safe_fs_access             2
#define LIND_safe_fs_unlink             4
#define LIND_safe_fs_link               5
#define LIND_safe_fs_rename             6

#define LIND_safe_fs_xstat              9
#define LIND_safe_fs_open               10
#define LIND_safe_fs_close              11
#define LIND_safe_fs_read               12
#define LIND_safe_fs_write              13
#define LIND_safe_fs_lseek              14
#define LIND_safe_fs_ioctl              15
#define LIND_safe_fs_truncate           16
#define LIND_safe_fs_fxstat             17
#define LIND_safe_fs_ftruncate          18
#define LIND_safe_fs_fstatfs            19
#define LIND_safe_fs_mmap               21
#define LIND_safe_fs_munmap             22
#define LIND_safe_fs_getdents           23
#define LIND_safe_fs_dup                24
#define LIND_safe_fs_dup2               25
#define LIND_safe_fs_statfs             26
#define LIND_safe_fs_fcntl              28

#define LIND_safe_sys_getppid           29
#define LIND_safe_sys_exit              30
#define LIND_safe_sys_getpid            31

#define LIND_safe_net_bind              33
#define LIND_safe_net_send              34
#define LIND_safe_net_sendto            35
#define LIND_safe_net_recv              36
#define LIND_safe_net_recvfrom          37
#define LIND_safe_net_connect           38
#define LIND_safe_net_listen            39
#define LIND_safe_net_accept            40

#define LIND_safe_net_getsockopt        43
#define LIND_safe_net_setsockopt        44
#define LIND_safe_net_shutdown          45
#define LIND_safe_net_select            46
#define LIND_safe_fs_getcwd             47
#define LIND_safe_net_poll              48
#define LIND_safe_net_socketpair        49
#define LIND_safe_sys_getuid            50
#define LIND_safe_sys_geteuid           51
#define LIND_safe_sys_getgid            52
#define LIND_safe_sys_getegid           53
#define LIND_safe_fs_flock              54

#define LIND_safe_net_epoll_create      56
#define LIND_safe_net_epoll_ctl         57
#define LIND_safe_net_epoll_wait        58

#define LIND_safe_fs_shmget             62
#define LIND_safe_fs_shmat              63
#define LIND_safe_fs_shmdt              64
#define LIND_safe_fs_shmctl             65

#define LIND_safe_fs_pipe               66
#define LIND_safe_fs_pipe2              67
#define LIND_safe_fs_fork               68
#define LIND_safe_fs_exec               69

#define LIND_safe_mutex_create          70
#define LIND_safe_mutex_destroy         71
#define LIND_safe_mutex_lock            72
#define LIND_safe_mutex_trylock         73
#define LIND_safe_mutex_unlock          74
#define LIND_safe_cond_create           75
#define LIND_safe_cond_destroy          76
#define LIND_safe_cond_wait             77
#define LIND_safe_cond_broadcast        78
#define LIND_safe_cond_signal           79
#define LIND_safe_cond_timedwait        80

#define LIND_safe_sem_init               91
#define LIND_safe_sem_wait               92
#define LIND_safe_sem_trywait            93
#define LIND_safe_sem_timedwait          94
#define LIND_safe_sem_post               95
#define LIND_safe_sem_destroy            96
#define LIND_safe_sem_getvalue           97

#define LIND_safe_net_gethostname       125

#define LIND_safe_fs_pread              126
#define LIND_safe_fs_pwrite             127
#define LIND_safe_fs_chdir              130
#define LIND_safe_fs_mkdir              131
#define LIND_safe_fs_rmdir              132
#define LIND_safe_fs_chmod              133
#define LIND_safe_fs_fchmod             134

#define LIND_safe_net_socket            136

#define LIND_safe_net_getsockname       144
#define LIND_safe_net_getpeername       145
#define LIND_safe_net_getifaddrs        146
#define LIND_safe_sys_sigaction		    147
#define LIND_safe_sys_kill		        148
#define LIND_safe_sys_sigprocmask	    149
#define LIND_safe_sys_lindsetitimer	    150


#define LIND_safe_fs_fchdir             161
#define LIND_safe_fs_fsync              162
#define LIND_safe_fs_fdatasync          163
#define LIND_safe_fs_sync_file_range    164


union RustArg {
    int dispatch_int;
    unsigned int dispatch_uint;
    int *dispatch_intptr;
    long unsigned int dispatch_ulong;
    long long unsigned int dispatch_ulong_long;
    long int dispatch_long;
    size_t dispatch_size_t;
    ssize_t dispatch_ssize_t;
    off_t dispatch_off_t;
    socklen_t dispatch_socklen_t;
    socklen_t *dispatch_socklen_t_ptr;
    const void *dispatch_cbuf;
    void *dispatch_mutcbuf;
    const char *dispatch_cstr;
    const char *const *dispatch_cstrarr;
    struct rlimit *strdispatch_rlimitstruct;
    struct lind_stat *dispatch_statstruct;
    struct statfs *dispatch_statfsstruct;
    struct timeval *dispatch_timevalstruct;
    struct timespec *dispatch_timespecstruct;
    struct sockaddr *dispatch_sockaddrstruct;
    struct epoll_event *dispatch_epolleventstruct;
    const struct sockaddr *dispatch_constsockaddrstruct;
    struct lind_shmid_ds *dispatch_shmidstruct;
    int *dispatch_pipearray;
    struct nacl_abi_sigaction *dispatch_naclabisigactionstruct;
    const struct nacl_abi_sigaction *dispatch_constnaclabisigactionstruct;
    uint64_t *dispatch_naclsigset;
    const uint64_t *dispatch_constnaclsigset;
    struct itimerval *dispatch_structitimerval;
    const struct itimerval *dispatch_conststructitimerval;
    fd_set *fdset;
};

int dispatcher(unsigned long int cageid, int callnum, union RustArg arg1, union RustArg arg2,
               union RustArg arg3, union RustArg arg4, union RustArg arg5, union RustArg arg6);

int quick_write(int fd, const void *buf, size_t count, int cageid);
int quick_read(int fd, void *buf, int size, int cageid);
void lindcancelinit(unsigned long int cageid);
void lindsetthreadkill(unsigned long int cageid, unsigned long int pthreadid, bool kill);   
bool lindcheckthread(unsigned long int cageid, unsigned long int pthreadid);
void lindthreadremove(unsigned long int cageid, unsigned long int pthreadid);
void lindrustinit(int verbosity);
void lindrustfinalize(void);

int lind_pread(int fd, void *buf, size_t count, off_t offset, int cageid);
int lind_pwrite(int fd, const void *buf, size_t count, off_t offset, int cageid);
int lind_unlink (const char *name, int cageid);
int lind_link (const char *from, const char *to, int cageid);
int lind_rename (const char *oldpath, const char *newpath, int cageid);
int lind_access (const char *file, int mode, int cageid);
int lind_truncate (const char *file, int length, int cageid);
int lind_ftruncate (int fd, int length, int cageid);
int lind_chdir (const char *name, int cageid);
int lind_mkdir (const char *path, int mode, int cageid);
int lind_rmdir (const char *path, int cageid);
int lind_chmod (const char *path, int mode, int cageid);
int lind_fchmod (int fd, int mode, int cageid);

int lind_fchdir(int fd, int cageid);
int lind_fsync(int fd, int cageid);
int lind_fdatasync(int fd, int cageid);
int lind_sync_file_range(int fd, off_t offset, off_t nbytes, unsigned int flags, int cageid);


int lind_xstat (const char *path, struct lind_stat *buf, int cageid);
int lind_open (const char *path, int flags, int mode, int cageid);
int lind_close (int fd, int cageid);
int lind_read (int fd, void *buf, int size, int cageid);
int lind_write (int fd, const void *buf, size_t count, int cageid);
int lind_lseek (int fd, off_t offset, int whence, int cageid);
int lind_fxstat (int fd, struct lind_stat *buf, int cageid);
int lind_fstatfs (int fd, struct statfs *buf, int cageid);
int lind_statfs (const char *path, struct statfs *buf, int cageid);
int lind_dup (int oldfd, int cageid);
int lind_dup2 (int oldfd, int newfd, int cageid);
int lind_getdents (int fd, char *buf, size_t nbytes, int cageid);
int lind_fcntl_get (int fd, int cmd, int cageid);
int lind_fcntl_set (int fd, int cmd, long set_op, int cageid);
int lind_ioctl (int fd, unsigned long request, void *arg_ptr, int cageid);
int lind_bind (int sockfd, const struct sockaddr *addr, socklen_t addrlen, int cageid);
int lind_send (int sockfd, const void *buf, size_t len, int flags, int cageid);
int lind_recv (int sockfd, void *buf, size_t len, int flags, int cageid);
int lind_sendto (int sockfd, const void *buf, size_t len, int flags, const struct sockaddr *dest_addr, socklen_t addrlen, int cageid);
int lind_recvfrom (int sockfd, const void *buf, size_t len, int flags, struct sockaddr *src_addr, socklen_t *addrlen,  int cageid);
int lind_connect (int sockfd, const struct sockaddr *src_addr, socklen_t addrlen, int cageid);
int lind_accept(int sockfd, struct sockaddr *sockaddr, socklen_t *addrlen, int cageid);
int lind_listen (int sockfd, int backlog, int cageid);
int lind_getsockopt (int sockfd, int level, int optname, void *optval, socklen_t *optlen, int cageid);
int lind_setsockopt (int sockfd, int level, int optname, const void *optval, socklen_t optlen, int cageid);
int lind_select (int nfds, fd_set * readfds, fd_set * writefds, fd_set * exceptfds, struct timeval *timeout, int cageid);
int lind_poll (struct pollfd *fds, nfds_t nfds, int timeout, int cageid);
int lind_epoll_create(int size, int cageid);
int lind_epoll_ctl(int epfd, int op, int fd, struct epoll_event *event, int cageid);
int lind_epoll_wait(int epfd, struct epoll_event *events, int maxevents, int timeout, int cageid);
int lind_getcwd (char *buf, size_t size, int cageid);
int lind_gethostname (char *name, size_t len, int cageid);
int lind_getpeername (int sockfd, struct sockaddr *addr, socklen_t *addrlen, int cageid);
int lind_getifaddrs (char *buf, size_t len, int cageid);
int lind_getsockname (int sockfd, struct sockaddr *addr, socklen_t *addrlen, int cageid);
int lind_socket (int domain, int type, int protocol, int cageid);
int lind_shutdown (int sockfd, int how, int cageid);
int lind_socketpair (int domain, int type, int protocol, int *fds, int cageid);
int lind_mutex_create (int cageid);
int lind_mutex_destroy (int mutex_handle, int cageid);
int lind_mutex_lock (int mutex_handle, int cageid);
int lind_mutex_trylock (int mutex_handle, int cageid);
int lind_mutex_unlock (int mutex_handle, int cageid);
int lind_cond_create (int cageid);
int lind_cond_destroy (int cond_handle, int cageid);
int lind_cond_wait (int cond_handle, int mutex_handle, int cageid);
int lind_cond_broadcast (int cond_handle, int cageid);
int lind_cond_signal (int cond_handle, int cageid);
int lind_cond_timedwait (int cond_handle, int mutex_handle, struct timespec *ts, int cageid);
int lind_sem_init (unsigned int sem, int pshared, int value, int cageid);
int lind_sem_wait (unsigned int sem, int cageid);
int lind_sem_trywait (unsigned int sem, int cageid);
int lind_sem_timedwait (unsigned int sem, struct timespec *abs, int cageid);
int lind_sem_post (unsigned int sem, int cageid);
int lind_sem_destroy (unsigned int sem, int cageid);
int lind_sem_getvalue (unsigned int sem, int cageid);
int lind_getuid (int cageid);
int lind_geteuid (int cageid);
int lind_getgid (int cageid);
int lind_getegid (int cageid);
int lind_flock (int fd, int operation, int cageid);
int lind_pipe(int* pipefds, int cageid);
int lind_pipe2(int* pipefds, int flags, int cageid);
int lind_fork(int newcageid, int cageid);
int lind_mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset, int cageid);
int lind_munmap(void *addr, size_t length, int cageid);
int lind_shmget(key_t key, size_t size, int shmflg, int cageid);
int lind_shmat(int shmid, void *shmaddr, int shmflg, int cageid);
int lind_shmdt(void *shmaddr, int cageid);
int lind_shmctl(int shmid, int cmd, struct lind_shmid_ds *buf, int cageid);
int lind_getpid(int cageid);
int lind_getppid(int cageid);
int lind_exec(int newcageid, int cageid);
int lind_exit(int status, int cageid);
int lind_sigaction(int sig, const struct nacl_abi_sigaction *act, struct nacl_abi_sigaction *ocat, int cageid);
int lind_kill(int targetcageid, int sig, int cageid);
int lind_sigprocmask(int how, const uint64_t *nacl_set, uint64_t *nacl_oldset, int cageid);
int lind_lindsetitimer(int which, const struct itimerval *new_value, struct itimerval *old_value, int cageid);


#endif /* LIND_PLATFORM_H_ */

