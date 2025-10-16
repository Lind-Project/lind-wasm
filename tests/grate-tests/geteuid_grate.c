#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <register_handler.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <errno.h>

// Function ptr and signatures of this grate
typedef int (*grate_fn_t)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
int geteuid_grate(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);

enum {
    SYS_geteuid = 107,    
    SYSCALL_MAX_NUM = 512 // capacity upper bound; set to platform max+1
};

// Default fallback handler for syscalls that have not been implemented
// or registered by this Grate instance. Always returns `-ENOSYS`, signaling 
// “Function not implemented.”
//
// -ENOSYS  Always returned.
static int grate_enosys(
    uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage
){
    return -ENOSYS;
}

// All entries are initialized to a default stub (`grate_enosys`) that 
// returns `-ENOSYS`
static grate_fn_t func_table[SYSCALL_MAX_NUM] = {
    [0 ... SYSCALL_MAX_NUM-1] = grate_enosys
};

// Returns the registered Grate handler for the given syscall number.
//
// Performs a simple bounds check to ensure the syscall number is valid.
// If out of range, returns `NULL`, which upstream dispatchers treat
// as an unimplemented syscall.
static inline grate_fn_t grate_lookup(uint64_t sysno) {
    if (sysno >= SYSCALL_MAX_NUM) return NULL;
    return func_table[sysno];
}

// Automatically executed (before `main()`) to populate the Grate’s
// per-syscall dispatch table (`func_table`).
//
// By default, every syscall slot is initialized to `grate_enosys`.
// This constructor selectively replaces the slot for `SYS_geteuid`
// with the Grate-specific implementation `geteuid_grate()`.
__attribute__((constructor))
static void grate_register_table(void) {
    func_table[SYS_geteuid] = geteuid_grate;
}

// Dispatcher function
int pass_fptr_to_wt(uint64_t sysno, uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage) {
    if (sysno < 0) {
        fprintf(stderr, "Invalid index: %llu\n", sysno);
        return -1; 
    }

    printf("[Grate | geteuid] Handling syscall number: %llu from cage: %llu\n", sysno, cageid);
    grate_fn_t fn = grate_lookup(sysno);
    if (!fn) return -ENOSYS;               // or call grate_enosys(...) if you want logging
    return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage, arg4, arg4cage, arg5, arg5cage, arg6, arg6cage);
}

int geteuid_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage) {
    return 10;
}

// Main function will always be same in all grates
int main(int argc, char* argv[]) {
    // Should be at least two inputs (at least one grate file and one cage file)
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_file> <grate_file> <cage_file> [...]\n", argv[0]);
        exit(EXIT_FAILURE);
    }

    int grateid = getpid();

    // Because we assume that all cages are unaware of the existence of grate, cages will not handle the logic of `exec`ing
    // grate, so we need to handle these two situations separately in grate.
    // grate needs to fork in two situations:
    // - the first is to fork and use its own cage;
    // - the second is when there is still at least one grate in the subsequent command line input.
    // In the second case, we fork & exec the new grate and let the new grate handle the subsequent process.
    for (int i = 1; i < (argc < 3 ? argc : 3); i++) {
        pid_t pid = fork();
        if (pid < 0) {
            perror("fork failed");
            exit(EXIT_FAILURE);
        } else if (pid == 0) {
            // According to input format, the odd-numbered positions will always be grate, and even-numbered positions
            // will always be cage.
            if (i % 2 != 0) {
                // Next one is cage, only set the register_handler when next one is cage
                int cageid = getpid();
                // Set the geteuid (syscallnum=107) of this cage to call this grate function geteuid_grate (func index=0)
                // Syntax of register_handler: <targetcage, targetcallnum, handlefunc_index_in_this_grate (non-zero), this_grate_id>
                int ret = register_handler(cageid, 107, 1, grateid);
            }

            if ( execv(argv[i], &argv[i]) == -1) {
                perror("execv failed");
                exit(EXIT_FAILURE);
            }
	}
    }

    int status;
    while (wait(&status) > 0) {
        printf("[Grate | geteuid] terminated, status: %d\n", status);
    }

    return 0;
}
