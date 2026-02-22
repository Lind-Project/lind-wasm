/* Test locale and timezone functionality in lind-wasm */

#include <assert.h>
#include <ctype.h>
#include <locale.h>
#include <string.h>
#include <time.h>
#include <stdlib.h>
#include <unistd.h>

int main(void) {
    /* --- C locale basics --- */

    /* setlocale should return "C" for default locale */
    char *loc = setlocale(LC_ALL, NULL);
    assert(loc != NULL);
    assert(strcmp(loc, "C") == 0 || strcmp(loc, "POSIX") == 0);

    /* Explicitly setting C locale should succeed */
    loc = setlocale(LC_ALL, "C");
    assert(loc != NULL);

    /* POSIX locale should also work (alias for C) */
    loc = setlocale(LC_ALL, "POSIX");
    assert(loc != NULL);

    /* --- ctype in C locale --- */

    assert(toupper('a') == 'A');
    assert(tolower('Z') == 'z');
    assert(isblank(' '));
    assert(isblank('\t'));
    assert(!isblank('\n'));

    /* --- localeconv in C locale --- */

    struct lconv *lc = localeconv();
    assert(lc != NULL);
    assert(strcmp(lc->decimal_point, ".") == 0);

    /* --- strftime in C locale --- */

    struct tm t = {0};
    t.tm_year = 124;  /* 2024 */
    t.tm_mon = 0;     /* January */
    t.tm_mday = 15;
    t.tm_wday = 1;    /* Monday */
    char buf[64];
    size_t n = strftime(buf, sizeof(buf), "%A", &t);
    assert(n > 0);
    assert(strcmp(buf, "Monday") == 0);

    /* --- timezone with explicit TZ --- */

    setenv("TZ", "UTC0", 1);
    tzset();

    time_t epoch = 0;
    struct tm *gm = gmtime(&epoch);
    assert(gm != NULL);
    assert(gm->tm_year == 70);  /* 1970 */
    assert(gm->tm_mon == 0);
    assert(gm->tm_mday == 1);
    assert(gm->tm_hour == 0);

    /* localtime with TZ=UTC0 should match gmtime */
    struct tm *lt = localtime(&epoch);
    assert(lt != NULL);
    assert(lt->tm_hour == 0);

    /* TZ with offset */
    setenv("TZ", "EST5", 1);
    tzset();
    lt = localtime(&epoch);
    assert(lt != NULL);
    assert(lt->tm_hour == 19);  /* UTC 0 - 5 = 19:00 previous day */

    /* --- non-C locale (expected to fail gracefully) --- */

    loc = setlocale(LC_ALL, "en_US.UTF-8");
    /* May return NULL if locale data not installed — that's ok */
    /* Just verify it doesn't crash */

    write(1, "done\n", 5);
    return 0;
}
