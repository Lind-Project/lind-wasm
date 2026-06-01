#include <errno.h>
#include <lind_syscall.h>

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

// Dispatcher function
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
		    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
		    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
		    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
		    uint64_t arg6, uint64_t arg6cage) {
	if (fn_ptr_uint == 0) {
		fprintf(stderr,
			"[Grate|diff-cage-args] Invalid function ptr\n");
		assert(0);
	}

	printf("[Grate|diff-cage-args] Handling function ptr: %llu from cage: "
	       "%llu\n",
	       fn_ptr_uint, cageid);

	int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		  uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		  uint64_t) =
	    (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		     uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
		     uint64_t))(uintptr_t)fn_ptr_uint;

	return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage, arg4,
		  arg4cage, arg5, arg5cage, arg6, arg6cage);
}

int read_grate(uint64_t grateid, uint64_t arg1, uint64_t arg1cage,
	       uint64_t arg2, uint64_t arg2cage, uint64_t arg3,
	       uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage,
	       uint64_t arg5, uint64_t arg5cage, uint64_t arg6,
	       uint64_t arg6cage) {
	int thiscage = getpid();
	int cageid = arg1cage;

	int fd = (int)arg1;
	int count = (size_t)arg3;

	ssize_t ret = 4321;

	char buf[11] = "helloworld";

	copy_data_between_cages(thiscage, arg2cage, (uint64_t)buf, thiscage,
				arg2, arg2cage, count,
				0 // Use copytype 0 so read exactly count
				  // bytes instead of stopping at '\0'
	);

	return ret;
}

int open_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2,
	       uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage,
	       uint64_t arg4, uint64_t arg4cage, uint64_t arg5,
	       uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage) {
	printf(
	    "[Grate|diff-cage-args] In open_grate %d handler for cage: %llu\n",
	    getpid(), cageid);

	int self_grate_id = getpid();

	// Overwrite the path supplied to open with a different path.
	char new_path[20] = "/tmp/redirected.txt";

	int ret = make_threei_call(
	    2, 0, self_grate_id, arg1cage,
	    // We need to modify the cageid here to indicate that we want the
	    // address translated.
	    (uint64_t)&new_path, self_grate_id | GRATE_MEMORY_FLAG, arg2, arg2cage,
	    arg3, arg3cage, arg4, arg4cage, arg5, arg5cage, arg6, arg6cage,
	    0 // we will handle the errno in this grate instead of translating
	      // it to
	);

	return ret;
}

// Main function will always be same in all grates
int main(int argc, char *argv[]) {
	// Should be at least one input (at least one grate file and one cage
	// file)
	if (argc < 2) {
		fprintf(stderr, "Usage: %s <cage_file> <grate_file>\n",
			argv[0]);
		assert(0);
	}

	int grateid = getpid();

	pid_t pid = fork();
	if (pid < 0) {
		perror("fork failed");
		assert(0);
	} else if (pid == 0) {
		int cageid = getpid();

		// This is to test whether we can use arg, argcage from
		// different cages.
		uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&open_grate;
		int ret = register_handler(cageid, 2, grateid, fn_ptr_addr);

		// This is to check copy_data for regression.
		fn_ptr_addr = (uint64_t)(uintptr_t)&read_grate;
		ret = register_handler(cageid, 0, grateid, fn_ptr_addr);

		if (execv(argv[1], &argv[1]) == -1) {
			perror("execv failed");
			assert(0);
		}
	}

	int status;
	int failed = 0;
	while (wait(&status) > 0) {
		if (status != 0) {
			fprintf(stderr,
				"[Grate|diff-cage-args] FAIL: child exited "
				"with status %d\n",
				status);
			assert(0);
		}
	}

	printf("[Grate|diff-cage-args] PASS\n");
	return 0;
}
