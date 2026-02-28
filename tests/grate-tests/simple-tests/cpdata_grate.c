// Grate side of the copy_data_between_cages test (issue #833).
// Intercepts write(1, buf, count) from the cage, mallocs a local buffer,
// uses copy_data_between_cages() to copy the cage's data into it, and
// verifies the contents match.
#include <errno.h>
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>

// Dispatcher function — required by 3i grate callback trampoline
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) {
        fprintf(stderr, "[Grate|cpdata] Invalid function ptr\n");
        assert(0);
    }

    // The handler for write() receives all 6 arg pairs
    int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t) =
        (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t))(uintptr_t)fn_ptr_uint;

    return fn(cageid, arg1, arg1cage, arg2, arg2cage,
              arg3, arg3cage, arg4, arg4cage,
              arg5, arg5cage, arg6, arg6cage);
}

// write() handler: intercepts write(fd, buf, count)
// arg1/arg1cage = fd, arg2/arg2cage = buf (host-translated), arg3/arg3cage = count
int write_grate(uint64_t cageid,
                uint64_t arg1, uint64_t arg1cage,
                uint64_t arg2, uint64_t arg2cage,
                uint64_t arg3, uint64_t arg3cage,
                uint64_t arg4, uint64_t arg4cage,
                uint64_t arg5, uint64_t arg5cage,
                uint64_t arg6, uint64_t arg6cage) {
    uint64_t src_host_addr = arg2;  // already host-translated by glibc
    uint64_t src_cage = arg2cage;   // cage that owns the buffer
    size_t count = (size_t)arg3;

    printf("[Grate|cpdata] Intercepted write: cage=%llu, buf=%llx, count=%zu\n",
           cageid, src_host_addr, count);

    // Allocate a local buffer via malloc — this is the pattern that triggered #833
    char *dest = (char *)malloc(count + 1);
    if (!dest) {
        fprintf(stderr, "[Grate|cpdata] malloc failed\n");
        assert(0);
    }
    memset(dest, 0, count + 1);

    uint64_t grate_cageid = (uint64_t)getpid();

    // Copy data from cage's buffer into grate's malloc'd buffer
    int ret = copy_data_between_cages(
        grate_cageid,       // thiscage
        src_cage,           // targetcage
        src_host_addr,      // srcaddr (host addr, passed through for foreign cage)
        src_cage,           // srccage
        (uint64_t)(uintptr_t)dest, // destaddr (user-space, will be translated)
        grate_cageid,       // destcage
        (uint64_t)count,    // len
        0                   // copytype = memcpy
    );
    if (ret < 0) {
        fprintf(stderr, "[Grate|cpdata] FAIL: copy_data_between_cages returned %d\n", ret);
        free(dest);
        assert(0);
    }

    // Verify the copied data
    if (memcmp(dest, "hello", count) != 0) {
        fprintf(stderr, "[Grate|cpdata] FAIL: data mismatch, got '%s'\n", dest);
        free(dest);
        assert(0);
    }
    printf("[Grate|cpdata] copy_data OK: '%s'\n", dest);

    free(dest);

    // Forward the original write to the actual syscall so cage gets correct return
    int self_grate_id = getpid();
    int write_ret = make_threei_call(
        1,  // syscallnum for write
        0, self_grate_id, self_grate_id,
        arg1, arg1cage,
        arg2, arg2cage,
        arg3, arg3cage,
        arg4, arg4cage,
        arg5, arg5cage,
        arg6, arg6cage,
        0  // no errno translation
    );
    return write_ret;
}

// Main function — same boilerplate as all grates
int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_file> [<grate_file> <cage_file> ...]\n",
                argv[0]);
        assert(0);
    }

    int grateid = getpid();

    for (int i = 1; i < (argc < 3 ? argc : 3); i++) {
        pid_t pid = fork();
        if (pid < 0) {
            perror("fork failed");
            assert(0);
        } else if (pid == 0) {
            if (i % 2 != 0) {
                int cageid = getpid();
                // Interpose write (syscall 1)
                uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&write_grate;
                printf("[Grate|cpdata] Registering write handler for cage %d in "
                       "grate %d with fn ptr addr: %llu\n",
                       cageid, grateid, fn_ptr_addr);
                int ret = register_handler(cageid, 1, grateid, fn_ptr_addr);
                if (ret != 0) {
                    fprintf(stderr, "[Grate|cpdata] Failed to register handler, ret: %d\n", ret);
                    assert(0);
                }
            }

            if (execv(argv[i], &argv[i]) == -1) {
                perror("execv failed");
                assert(0);
            }
        }
    }

    int status;
    while (wait(&status) > 0) {
        if (status != 0) {
            fprintf(stderr, "[Grate|cpdata] FAIL: child exited with status %d\n", status);
            assert(0);
        }
    }

    printf("[Grate|cpdata] PASS\n");
    return 0;
}
