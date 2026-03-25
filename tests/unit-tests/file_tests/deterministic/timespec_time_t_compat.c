#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <sys/stat.h>
#include <assert.h>

int main() {
    /* Verify struct timespec.tv_sec is time_t (POSIX requirement).
       This was broken when the installed header used __time64_t
       instead of time_t for tv_sec on wasm32. */

    /* 1. sizeof check: tv_sec must be the same size as time_t */
    struct timespec ts;
    assert(sizeof(ts.tv_sec) == sizeof(time_t));

    /* 2. Type compatibility: &ts.tv_sec must be assignable to time_t* */
    time_t *tp = &ts.tv_sec;
    (void)tp;

    /* 3. The actual use case that broke: stat + localtime */
    struct stat st;
    int ret = stat("/", &st);
    assert(ret == 0);

    struct tm *tm = localtime(&st.st_mtime);
    assert(tm != NULL);

    /* 4. Verify the time is reasonable (after year 2000) */
    assert(tm->tm_year >= 100);

    printf("timespec_time_t_compat: all checks passed\n");
    return 0;
}
