#ifndef STRACE_H
#define STRACE_H

#include <stdint.h>
#include <stdio.h>
#include <unistd.h>
#include <lind_syscall.h>

#define ARG_INT 0
#define ARG_STR 1
#define ARG_PTR 2
#define MAX_SYSCALLS 334

extern int tracing_enabled;
struct trace_entry {
    uint64_t syscall_num;
    uint64_t a1, a2, a3, a4, a5, a6;
    int ret;
};
extern struct trace_entry trace_buf[100000];
extern volatile int trace_idx;

// function ptr for storing syscall handlers
typedef int (*syscall_handler_t)(uint64_t, uint64_t, uint64_t, uint64_t,
                                 uint64_t, uint64_t, uint64_t, uint64_t,
                                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);

// table for storing syscall handlers
extern syscall_handler_t syscall_handler_table[MAX_SYSCALLS];

// macro for defining syscall handlers dynamically
#define DEFINE_HANDLER(name, num, ...)                                          \
    int name##_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage,         \
                     uint64_t arg2, uint64_t arg2cage,                          \
                     uint64_t arg3, uint64_t arg3cage,                          \
                     uint64_t arg4, uint64_t arg4cage,                          \
                     uint64_t arg5, uint64_t arg5cage,                          \
                     uint64_t arg6, uint64_t arg6cage) {                        \
                                                                                \
                                                                                \
        int idx = __sync_fetch_and_add(&trace_idx, 1); \
        if (idx < 100000) { \
            trace_buf[idx].syscall_num = num; \
            trace_buf[idx].a1 = arg1;\
            trace_buf[idx].a2 = arg2;\
            trace_buf[idx].a3 = arg3;\
            trace_buf[idx].a4 = arg4;\
            trace_buf[idx].a5 = arg5;\
            trace_buf[idx].a6 = arg6;\
        } \
        int ret = make_threei_call(num, 0,                                      \
                                   cageid, 777777,                            \
                                   arg1, arg1cage, arg2, arg2cage,              \
                                   arg3, arg3cage, arg4, arg4cage,              \
                                   arg5, arg5cage, arg6, arg6cage, 0);          \
                                                                                \
        if (idx < 100000) { \
            trace_buf[idx].ret = ret; \
        } \
        return ret;                                                             \
    }                                                                           \
                                                                                \
    __attribute__((constructor)) static void register_##name() {                \
        syscall_handler_table[num] = &name##_grate;                             \
    }

#endif
