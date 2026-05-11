#include <assert.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/*
 * Test that execve handles non-UTF-8 bytes in argv/envp without EFAULT.
 * Regression test for get_cstr rejecting non-UTF-8 via CStr::to_str().
 *
 * We call execve with a valid path that doesn't exist and non-UTF-8
 * argv/envp. Before the fix this returned EFAULT; after the fix it
 * should return ENOENT (file not found), proving the non-UTF-8 bytes
 * were accepted and the syscall progressed past argument parsing.
 */

int main(void)
{
    char non_utf8_arg[] = { 'a', (char)0xFF, (char)0xA7, 'z', '\0' };
    char non_utf8_env[] = { 'K', '=', (char)0xFE, (char)0x80, '\0' };

    char *argv[] = { "/no/such/binary", non_utf8_arg, NULL };
    char *envp[] = { non_utf8_env, NULL };

    int ret = execve("/no/such/binary", argv, envp);

    /* execve should fail with ENOENT, NOT EFAULT */
    assert(ret == -1);
    assert(errno != EFAULT);
    assert(errno == ENOENT);

    printf("exec_non_utf8: all tests passed\n");
    return 0;
}
