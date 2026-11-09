#include <errno.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static inline uint64_t ticks(void) {
    uint32_t lo, hi;
    __asm__ volatile("lfence\n\trdtsc\n\tlfence"
                     : "=a"(lo), "=d"(hi) : : "memory");
    return ((uint64_t)hi << 32) | lo;
}

__attribute__((noinline)) static uid_t local_return10(void) {
    __asm__ volatile("" : : : "memory");
    return 10;
}

static unsigned parse_count(const char *text) {
    char *end;
    errno = 0;
    unsigned long value = strtoul(text, &end, 10);
    if (errno || end == text || *end || value == 0 || value > 1000000) {
        fprintf(stderr, "Invalid count (expected 1..1000000): %s\n", text);
        exit(2);
    }
    return (unsigned)value;
}

int main(int argc, char **argv) {
    if (argc != 4) {
        fprintf(stderr, "Usage: %s <single samples> <batches> <batch size>\n", argv[0]);
        return 2;
    }
    unsigned singles = parse_count(argv[1]);
    unsigned batches = parse_count(argv[2]);
    unsigned size = parse_count(argv[3]);
    uint64_t *single = calloc(singles, sizeof(*single));
    uint64_t *empty = calloc(singles, sizeof(*empty));
    uint64_t *dispatch = calloc(batches, sizeof(*dispatch));
    uint64_t *control = calloc(batches, sizeof(*control));
    if (!single || !empty || !dispatch || !control) {
        perror("calloc");
        return 1;
    }
    for (unsigned i = 0; i < 20000; ++i) {
        if (geteuid() != 10) {
            fprintf(stderr, "geteuid handler must return 10\n");
            return 1;
        }
        local_return10();
    }
    for (unsigned i = 0; i < singles; ++i) {
        for (unsigned j = 0; j < 2; ++j) {
            uint64_t start = ticks();
            uid_t result = 10;
            if ((i + j) % 2 == 0)
                result = geteuid();
            uint64_t end = ticks();
            if (result != 10 || end <= start) {
                fprintf(stderr, "Invalid single-call result or timestamp\n");
                return 1;
            }
            if ((i + j) % 2 == 0)
                single[i] = end - start;
            else
                empty[i] = end - start;
        }
    }
    for (unsigned i = 0; i < batches; ++i) {
        for (unsigned j = 0; j < 2; ++j) {
            uint64_t sum = 0;
            uint64_t start = ticks();
            if ((i + j) % 2 == 0) {
                for (unsigned k = 0; k < size; ++k)
                    sum += geteuid();
            } else {
                for (unsigned k = 0; k < size; ++k)
                    sum += local_return10();
            }
            uint64_t end = ticks();
            if (sum != (uint64_t)10 * size || end <= start) {
                fprintf(stderr, "Invalid batch checksum or timestamp\n");
                return 1;
            }
            if ((i + j) % 2 == 0)
                dispatch[i] = end - start;
            else
                control[i] = end - start;
        }
    }
    puts("kind,index,dispatch_ticks,control_ticks,calls");
    for (unsigned i = 0; i < singles; ++i)
        printf("single,%u,%" PRIu64 ",%" PRIu64 ",1\n", i, single[i], empty[i]);
    for (unsigned i = 0; i < batches; ++i)
        printf("batch,%u,%" PRIu64 ",%" PRIu64 ",%u\n",
               i, dispatch[i], control[i], size);
    free(single);
    free(empty);
    free(dispatch);
    free(control);
    return 0;
}
