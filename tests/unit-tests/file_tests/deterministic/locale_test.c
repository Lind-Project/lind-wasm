/* Test locale, langinfo, and timezone functionality in lind-wasm */

#include <assert.h>
#include <ctype.h>
#include <langinfo.h>
#include <locale.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

int main(void) {

    /* ===== setlocale ===== */

    /* Query default — should be "C" or "POSIX" */
    char *loc = setlocale(LC_ALL, NULL);
    assert(loc != NULL);
    assert(strcmp(loc, "C") == 0 || strcmp(loc, "POSIX") == 0);

    /* Explicitly set C */
    loc = setlocale(LC_ALL, "C");
    assert(loc != NULL);

    /* POSIX is an alias for C */
    loc = setlocale(LC_ALL, "POSIX");
    assert(loc != NULL);

    /* Per-category query */
    loc = setlocale(LC_CTYPE, NULL);
    assert(loc != NULL);
    loc = setlocale(LC_NUMERIC, NULL);
    assert(loc != NULL);
    loc = setlocale(LC_TIME, NULL);
    assert(loc != NULL);
    loc = setlocale(LC_MONETARY, NULL);
    assert(loc != NULL);

    /* en_US.UTF-8 should now be available */
    loc = setlocale(LC_ALL, "en_US.UTF-8");
    assert(loc != NULL);
    assert(strcmp(nl_langinfo(CODESET), "UTF-8") == 0);

    /* Restore C locale */
    setlocale(LC_ALL, "C");

    /* ===== localeconv (C locale) ===== */

    struct lconv *lc = localeconv();
    assert(lc != NULL);
    assert(strcmp(lc->decimal_point, ".") == 0);
    assert(strcmp(lc->thousands_sep, "") == 0);
    assert(strcmp(lc->grouping, "") == 0);
    assert(strcmp(lc->int_curr_symbol, "") == 0);
    assert(strcmp(lc->currency_symbol, "") == 0);
    assert(strcmp(lc->mon_decimal_point, "") == 0);
    assert(strcmp(lc->mon_thousands_sep, "") == 0);
    assert(strcmp(lc->positive_sign, "") == 0);
    assert(strcmp(lc->negative_sign, "") == 0);
    assert(lc->frac_digits == 127);       /* CHAR_MAX in C locale */
    assert(lc->int_frac_digits == 127);

    /* ===== nl_langinfo (C locale) ===== */

    /* Codeset */
    char *cs = nl_langinfo(CODESET);
    assert(cs != NULL);
    assert(strlen(cs) > 0);  /* "ANSTRUTS-1" or "ASCII" etc. */

    /* Day names */
    char *day = nl_langinfo(DAY_1);  /* Sunday */
    assert(day != NULL);
    assert(strcmp(day, "Sunday") == 0);

    day = nl_langinfo(DAY_2);  /* Monday */
    assert(day != NULL);
    assert(strcmp(day, "Monday") == 0);

    /* Abbreviated day */
    char *abday = nl_langinfo(ABDAY_1);
    assert(abday != NULL);
    assert(strcmp(abday, "Sun") == 0);

    /* Month names */
    char *mon = nl_langinfo(MON_1);  /* January */
    assert(mon != NULL);
    assert(strcmp(mon, "January") == 0);

    char *abmon = nl_langinfo(ABMON_1);
    assert(abmon != NULL);
    assert(strcmp(abmon, "Jan") == 0);

    /* AM/PM */
    char *am = nl_langinfo(AM_STR);
    assert(am != NULL);
    assert(strcmp(am, "AM") == 0);
    char *pm = nl_langinfo(PM_STR);
    assert(pm != NULL);
    assert(strcmp(pm, "PM") == 0);

    /* Radix (decimal point) */
    char *radix = nl_langinfo(RADIXCHAR);
    assert(radix != NULL);
    assert(strcmp(radix, ".") == 0);

    /* Thousands separator */
    char *thou = nl_langinfo(THOUSEP);
    assert(thou != NULL);
    assert(strcmp(thou, "") == 0);

    /* Yes/No expressions */
    char *yesexpr = nl_langinfo(YESEXPR);
    assert(yesexpr != NULL);
    assert(strlen(yesexpr) > 0);
    char *noexpr = nl_langinfo(NOEXPR);
    assert(noexpr != NULL);
    assert(strlen(noexpr) > 0);

    /* ===== ctype (C locale) ===== */

    /* toupper/tolower full range */
    assert(toupper('a') == 'A');
    assert(toupper('z') == 'Z');
    assert(toupper('A') == 'A');   /* already upper */
    assert(toupper('5') == '5');   /* non-alpha unchanged */
    assert(tolower('Z') == 'z');
    assert(tolower('a') == 'a');   /* already lower */

    /* isblank */
    assert(isblank(' '));
    assert(isblank('\t'));
    assert(!isblank('\n'));
    assert(!isblank('a'));

    /* isalnum boundary */
    assert(isalnum('0'));
    assert(isalnum('9'));
    assert(isalnum('a'));
    assert(isalnum('Z'));
    assert(!isalnum(' '));
    assert(!isalnum('\0'));

    /* isprint vs iscntrl */
    assert(isprint(' '));
    assert(!isprint('\x01'));
    assert(iscntrl('\x01'));
    assert(iscntrl('\x7f'));  /* DEL */
    assert(!iscntrl('A'));

    /* ===== strftime (C locale) ===== */

    struct tm t = {0};
    t.tm_year = 124;    /* 2024 */
    t.tm_mon = 0;       /* January */
    t.tm_mday = 15;
    t.tm_hour = 14;
    t.tm_min = 30;
    t.tm_sec = 45;
    t.tm_wday = 1;      /* Monday */
    t.tm_yday = 14;
    char buf[128];

    /* Full day name */
    assert(strftime(buf, sizeof(buf), "%A", &t) > 0);
    assert(strcmp(buf, "Monday") == 0);

    /* Abbreviated day */
    assert(strftime(buf, sizeof(buf), "%a", &t) > 0);
    assert(strcmp(buf, "Mon") == 0);

    /* Full month name */
    assert(strftime(buf, sizeof(buf), "%B", &t) > 0);
    assert(strcmp(buf, "January") == 0);

    /* Abbreviated month */
    assert(strftime(buf, sizeof(buf), "%b", &t) > 0);
    assert(strcmp(buf, "Jan") == 0);

    /* Year */
    assert(strftime(buf, sizeof(buf), "%Y", &t) > 0);
    assert(strcmp(buf, "2024") == 0);

    /* ISO date */
    assert(strftime(buf, sizeof(buf), "%Y-%m-%d", &t) > 0);
    assert(strcmp(buf, "2024-01-15") == 0);

    /* Time */
    assert(strftime(buf, sizeof(buf), "%H:%M:%S", &t) > 0);
    assert(strcmp(buf, "14:30:45") == 0);

    /* AM/PM */
    assert(strftime(buf, sizeof(buf), "%p", &t) > 0);
    assert(strcmp(buf, "PM") == 0);

    /* ===== gmtime / localtime / mktime ===== */

    /* Epoch in UTC */
    setenv("TZ", "UTC0", 1);
    tzset();

    time_t epoch = 0;
    struct tm *gm = gmtime(&epoch);
    assert(gm != NULL);
    assert(gm->tm_year == 70);
    assert(gm->tm_mon == 0);
    assert(gm->tm_mday == 1);
    assert(gm->tm_hour == 0);
    assert(gm->tm_min == 0);
    assert(gm->tm_sec == 0);

    /* localtime with UTC should match gmtime */
    struct tm *lt = localtime(&epoch);
    assert(lt != NULL);
    assert(lt->tm_hour == 0);
    assert(lt->tm_mday == 1);

    /* EST = UTC-5 */
    setenv("TZ", "EST5", 1);
    tzset();
    lt = localtime(&epoch);
    assert(lt != NULL);
    assert(lt->tm_hour == 19);  /* Dec 31 1969, 19:00 EST */
    assert(lt->tm_mday == 31);
    assert(lt->tm_mon == 11);   /* December */
    assert(lt->tm_year == 69);  /* 1969 */

    /* Positive offset: UTC+9 (e.g. Japan) */
    setenv("TZ", "JST-9", 1);
    tzset();
    lt = localtime(&epoch);
    assert(lt != NULL);
    assert(lt->tm_hour == 9);   /* Jan 1 1970, 09:00 JST */
    assert(lt->tm_mday == 1);

    /* mktime round-trip */
    setenv("TZ", "UTC0", 1);
    tzset();
    struct tm input = {0};
    input.tm_year = 100;   /* 2000 */
    input.tm_mon = 5;      /* June */
    input.tm_mday = 15;
    input.tm_hour = 12;
    time_t result = mktime(&input);
    assert(result > 0);
    struct tm *check = gmtime(&result);
    assert(check->tm_year == 100);
    assert(check->tm_mon == 5);
    assert(check->tm_mday == 15);
    assert(check->tm_hour == 12);

    write(1, "done\n", 5);
    return 0;
}
