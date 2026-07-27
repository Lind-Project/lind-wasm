#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>

#define PAGESIZE 4096

static void assert_pipe_still_clean(int read_fd, int write_fd) {
    char canary = 'K';
    assert(write(write_fd, &canary, 1) == 1);
    char received = 0;
    assert(read(read_fd, &received, 1) == 1);
    assert(received == canary);
}

int main() {
    int fds[2];
    assert(pipe(fds) == 0);

    unsigned char *region = mmap(NULL, 2*PAGESIZE, PROT_READ | PROT_WRITE, 
        MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(region != MAP_FAILED);

    assert(munmap(region + PAGESIZE, PAGESIZE) == 0);

    // Positive control
    region[0] = 0xAB;
    errno = 0;
    ssize_t ret0 = write(fds[1], region, 1);
    assert(ret0 == 1);
    char received_control = 0;
    assert(read(fds[0], &received_control, 1) == 1);
    assert((unsigned char) received_control == 0xAB);

    // Test 1
    errno = 0;
    ssize_t ret1 = write(fds[1], region+PAGESIZE, PAGESIZE);
    assert(ret1 == -1);
    assert(errno == EFAULT);
    assert_pipe_still_clean(fds[0], fds[1]);

    // Test 2
    unsigned char *straddle_ptr = region+PAGESIZE-16;
    errno = 0;
    ssize_t ret2 = write(fds[1], straddle_ptr, 32);
    assert(ret2 == -1);
    assert(errno == EFAULT);
    assert_pipe_still_clean(fds[0], fds[1]);

    // Test 3
    
    int32_t neg_len = -1;
    size_t huge_count = (size_t)(uint32_t)neg_len;
    errno = 0;

    ssize_t ret3 = write(fds[1], straddle_ptr, huge_count);
    assert(ret3 == -1);
    assert(errno == EFAULT);
    assert_pipe_still_clean(fds[0], fds[1]);

    assert(munmap(region, PAGESIZE) == 0);
    close(fds[0]);
    close(fds[1]);

    printf("cross_cage_vmmap_escape test: PASS\n");
    return 0;
}
