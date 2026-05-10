/* Grate side of the mmap-with-GRATE_MEMORY_FLAG test.
 *
 * Registers an mmap handler that forwards the cage's mmap call to the
 * runtime via make_threei_call, with `addr_cage` tagged with
 * `GRATE_MEMORY_FLAG`.  This exercises the runtime's flag-aware path in
 * mmap_syscall (skip the truncate-and-translate-via-cage-vmmap step, treat
 * the addr as a host sysaddr when non-zero).
 *
 * The test uses MAP_ANONYMOUS|MAP_PRIVATE with addr=NULL; the runtime will
 * pick an address.  We're testing that the flag doesn't break the path, not
 * MAP_FIXED placement.
 */

#include <assert.h>
#include <errno.h>
#include <lind_syscall.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

/* Standard dispatcher used by every grate.  Unchanged from the other
   simple-tests grates. */
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
		    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
		    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
		    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
		    uint64_t arg6, uint64_t arg6cage) {
	if (fn_ptr_uint == 0) {
		fprintf(stderr, "[Grate|mmap-flag] Invalid function ptr\n");
		assert(0);
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

/* mmap interception.  Forward to RawPOSIX (MMAP_SYSCALL = 9) with the
   addr's cageid tagged with GRATE_MEMORY_FLAG, asserting the runtime
   accepts the flag and returns a usable cage uaddr. */
int mmap_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2,
	       uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage,
	       uint64_t arg4, uint64_t arg4cage, uint64_t arg5,
	       uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage) {
	int self_grate_id = getpid();

	/* Forward with arg1cage tagged GRATE_MEMORY_FLAG.  The cage's addr is
	   NULL (no MAP_FIXED) so the runtime picks; we're testing that the
	   flag-aware branch doesn't crash and returns the same useraddr the
	   non-flag branch would. */
	return make_threei_call(
	    9 /* MMAP_SYSCALL */, 0, self_grate_id, cageid, arg1,
	    self_grate_id | GRATE_MEMORY_FLAG, arg2, arg2cage, arg3, arg3cage,
	    arg4, arg4cage, arg5, arg5cage, arg6, arg6cage,
	    0 /* translate_errno off — propagate raw return */
	);
}

int main(int argc, char *argv[]) {
	if (argc < 2) {
		fprintf(stderr, "Usage: %s <cage_file>\n", argv[0]);
		assert(0);
	}

	int grateid = getpid();
	pid_t pid = fork();
	if (pid < 0) {
		perror("fork failed");
		assert(0);
	} else if (pid == 0) {
		int cageid = getpid();
		uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&mmap_grate;
		register_handler(cageid, 9 /* MMAP_SYSCALL */, grateid,
				 fn_ptr_addr);

		if (execv(argv[1], &argv[1]) == -1) {
			perror("execv failed");
			assert(0);
		}
	}

	int status;
	while (wait(&status) > 0) {
		if (status != 0) {
			fprintf(stderr,
				"[Grate|mmap-flag] FAIL: child exited with "
				"status %d\n",
				status);
			assert(0);
		}
	}

	printf("[Grate|mmap-flag] PASS\n");
	return 0;
}
