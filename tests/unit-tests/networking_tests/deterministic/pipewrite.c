#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

int main(void)
{
    const char *msg1 = "one\n";
    const char *msg2 = "two\n";
    const char *msg3 = "three\n";
    const size_t msg1_len = 4;
    const size_t msg2_len = 4;
    const size_t msg3_len = 6;
    const size_t total_len = msg1_len + msg2_len + msg3_len;
    const char *expected = "one\ntwo\nthree\n";
    
    char read_buf[total_len];
    int p[2];
    int ret;

    ret = pipe(p);
    assert(ret == 0);

    ret = write(p[1], msg1, msg1_len);
    assert(ret == (int)msg1_len);

    ret = write(p[1], msg2, msg2_len);
    assert(ret == (int)msg2_len);

    ret = write(p[1], msg3, msg3_len);
    assert(ret == (int)msg3_len);

    assert(total_len == strlen(expected));

    ret = read(p[0], read_buf, total_len);
    assert(ret == (int)total_len);

    assert(memcmp(read_buf, expected, total_len) == 0);

    ret = close(p[0]);
    assert(ret == 0);

    ret = close(p[1]);
    assert(ret == 0);

    return 0;
} 
