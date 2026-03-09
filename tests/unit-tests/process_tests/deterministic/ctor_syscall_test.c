/* Test that syscalls work from constructors (__attribute__((constructor))).
   This validates that __lind_init_addr_translation runs before user
   constructors via .init_array priority ordering.  Without the fix,
   clock_gettime called from a constructor would panic in 3i because
   __lind_cageid is still 0.  See issue #883.  */

#include <assert.h>
#include <time.h>

static int ctor_ran = 0;

__attribute__((constructor))
static void my_early_init(void)
{
    /* This syscall requires __lind_cageid to be initialized.
       Before the fix it would panic with "cage 0" in 3i.  */
    struct timespec ts;
    int ret = clock_gettime(CLOCK_MONOTONIC, &ts);
    assert(ret == 0);
    assert(ts.tv_sec >= 0);
    ctor_ran = 1;
}

int main(void)
{
    assert(ctor_ran == 1);
    return 0;
}
