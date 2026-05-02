#include <errno.h>
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>

// Dispatcher function
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
		    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
		    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
		    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
		    uint64_t arg6, uint64_t arg6cage) {
	if (fn_ptr_uint == 0) {
		return -1;
	}

	int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		  uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		  uint64_t) =
	    (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		     uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		     uint64_t))(uintptr_t)fn_ptr_uint;

	return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage, arg4,
		  arg4cage, arg5, arg5cage, arg6, arg6cage);
}

int open_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2,
	       uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage,
	       uint64_t arg4, uint64_t arg4cage, uint64_t arg5,
	       uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage) {
	int thiscage = getpid();

    printf("[Grate|open] intercepts open call: thiscage=%d, arg1cage=%llu\n", thiscage, arg1cage);

	char *pathname = malloc(4096);

	if (pathname == NULL) {
		perror("malloc failed");
		assert(0);
	}

    // Must use strncpy here to avoid reading invalid memory after the null terminator
	copy_data_between_cages(thiscage, arg1cage, arg1, arg1cage,
				(uint64_t)pathname, thiscage, 4096, 1);

    printf("[Grate|open] copied pathname: %s\n", pathname);

    if (strcmp(pathname, "random") != 0) {
        fprintf(stderr, "[Grate|open] FAIL: expected pathname 'random', got '%s'\n", pathname);
        free(pathname);
        assert(0);
    }

	free(pathname);

    // We return arbitrary value here since we're just testing 
    // that the grate can copy the data correctly
    // This is only a simple test case for only testing the 
    // copy_data_between_cages() function, so we don't need to 
    // perform the actual open call for "random".
	return 10;
}

int main(int argc, char *argv[]) {
	if (argc < 2) {
		assert(0);
	}

	int grateid = getpid();

	pid_t pid = fork();
	if (pid < 0) {
		perror("fork failed");
		assert(0);
	} else if (pid == 0) {
		int cageid = getpid();

        // Set the open (syscallnum=2) of this cage to call this grate
        // function open_grate 
        // Syntax of register_handler:
        // <targetcage, targetcallnum, this_grate_id, fn_ptr_u64)>
		uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&open_grate;
		int ret = register_handler(cageid, 2, grateid, fn_ptr_addr);

		if (execv(argv[1], &argv[1]) == -1) {
			perror("execv failed");
			assert(0);
		}
	}

	int status;
    int failed = 0;
    while (wait(&status) > 0) {
        if (status != 0) {
        fprintf(stderr, "[Grate|open] FAIL: child exited with status %d\n", status);
        assert(0);
        }
    }

	return 0;
}
