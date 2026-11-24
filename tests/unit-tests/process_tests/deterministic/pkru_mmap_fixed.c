#include <assert.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>

enum { REPLACEMENTS = 2000 };

static volatile unsigned char *mapping;
static atomic_int reader_started;
static atomic_int done;

static void *read_mapping(void *unused) {
    (void)unused;
    atomic_store(&reader_started, 1);
    while (!atomic_load(&done)) {
        (void)*mapping;
    }
    return NULL;
}

int main(void) {
    long page_size = sysconf(_SC_PAGESIZE);
    assert(page_size > 0);
    mapping = mmap(NULL, (size_t)page_size, PROT_READ | PROT_WRITE,
                   MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(mapping != MAP_FAILED);

    pthread_t reader;
    assert(pthread_create(&reader, NULL, read_mapping, NULL) == 0);
    while (!atomic_load(&reader_started)) {
        assert(sched_yield() == 0);
    }

    for (int i = 0; i < REPLACEMENTS; ++i) {
        void *result = mmap((void *)mapping, (size_t)page_size, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
        assert(result == (void *)mapping);
    }
    atomic_store(&done, 1);
    assert(pthread_join(reader, NULL) == 0);
    assert(munmap((void *)mapping, (size_t)page_size) == 0);
    puts("fixed mappings passed");
    return 0;
}
