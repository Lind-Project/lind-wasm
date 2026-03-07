#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include "strace.h"

struct trace_entry trace_buf[100000];
volatile int trace_idx = 0;

void dump_trace(void) {
    int n = trace_idx;
    if (n > 100000) {
        n = 100000;
    }

    for (int i = 0; i < n; i++) {
        printf("[%d] syscall=%lu args=(%lu, %lu, %lu, %lu, %lu, %lu) ret=%d\n",
               i,
               (unsigned long)trace_buf[i].syscall_num,
               (unsigned long)trace_buf[i].a1,
               (unsigned long)trace_buf[i].a2,
               (unsigned long)trace_buf[i].a3,
               (unsigned long)trace_buf[i].a4,
               (unsigned long)trace_buf[i].a5,
               (unsigned long)trace_buf[i].a6,
               trace_buf[i].ret);
    }
}
__attribute__((destructor))
static void dump_trace_at_exit(void) {
    fprintf(stderr, "[Grate] destructor: trace_idx=%d\n", trace_idx);
    dump_trace();
}

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_binary> [args...]\n", argv[0]);
        exit(EXIT_FAILURE);
    }

    int grateid = getpid();
    pid_t pid = fork();

    if (pid < 0) {
        perror("fork failed");
        exit(EXIT_FAILURE);
    } else if (pid == 0) {
        int cageid = getpid();
        for (int i = 0; i < MAX_SYSCALLS; i++) {
            if (syscall_handler_table[i] != NULL) {
                uint64_t fn_ptr = (uint64_t)(uintptr_t)syscall_handler_table[i];
                register_handler(cageid, i, grateid, fn_ptr);
            }
        }
        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv failed");
            exit(EXIT_FAILURE);
        }
    }

    int status;
    while (wait(&status) > 0) {
	fprintf(stderr, "[Grate] process terminated, status: %d\n", status);
    }

    return 0;
}
