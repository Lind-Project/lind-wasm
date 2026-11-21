/*
 * Test that fwrite() calls larger than the free stdio buffer space keep
 * their tail bytes across a following fseek().
 *
 * _IO_new_file_xsputn writes whole blocks directly and leaves the
 * remainder (n % BUFSIZ) in the stdio buffer. That remainder is only
 * flushed if the stream is in put mode, which _IO_OVERFLOW sets. Without
 * it, the next fseek() silently drops the tail and computes a wrong
 * in-buffer seek offset.
 *
 * The write pattern mimics how BFD (GNU as/ld) writes object files:
 * seek to a section offset, then fwrite a large block.
 */

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define FILE_SIZE 42000

static unsigned char expected[FILE_SIZE];

static void fill(unsigned char *buf, size_t n, unsigned seed) {
    for (size_t i = 0; i < n; i++)
        buf[i] = (unsigned char)((i * 7 + seed) & 0xff);
}

/* Write n bytes of pattern `seed` at offset `off`, and mirror it in memory. */
static void write_at(FILE *f, long off, size_t n, unsigned seed) {
    unsigned char *buf = malloc(n);
    assert(buf != NULL);
    fill(buf, n, seed);
    assert(fseek(f, off, SEEK_SET) == 0);
    assert(fwrite(buf, 1, n, f) == n);
    memcpy(expected + off, buf, n);
    free(buf);
}

int main(void) {
    const char *path = "fwrite_fseek_tail.out";
    FILE *f = fopen(path, "w+b");
    assert(f != NULL);

    /* Placeholder header, section data, then patch the header at the end. */
    write_at(f, 0, 64, 0);
    write_at(f, 64, 3000, 1);
    write_at(f, 16000, 9400, 2);   /* larger than one buffer: 8192 + 1208 tail */
    write_at(f, 3064, 5000, 3);    /* seek back into already-written data */
    write_at(f, 30000, 12000, 4);  /* 8192 + 3808 tail */
    write_at(f, 0, 64, 5);
    assert(fclose(f) == 0);

    /* Read the whole file back and compare byte for byte. */
    unsigned char *actual = malloc(FILE_SIZE + 1);
    assert(actual != NULL);
    f = fopen(path, "rb");
    assert(f != NULL);
    size_t got = fread(actual, 1, FILE_SIZE + 1, f);
    fclose(f);
    remove(path);

    printf("file size: %zu (expected %d)\n", got, FILE_SIZE);
    assert(got == FILE_SIZE);
    assert(memcmp(actual, expected, FILE_SIZE) == 0);
    free(actual);

    printf("fwrite tail survives fseek: ok\n");
    return 0;
}
