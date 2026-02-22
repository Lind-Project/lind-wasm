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

    /* ===== setlocale: C locale basics ===== */

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

    /* Non-existent locale should return NULL without crashing */
    loc = setlocale(LC_ALL, "xx_XX.FAKE-42");
    assert(loc == NULL);
    /* Locale should still be C after failed setlocale */
    loc = setlocale(LC_ALL, NULL);
    assert(loc != NULL);
    assert(strcmp(loc, "C") == 0 || strcmp(loc, "POSIX") == 0);

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
    assert(strlen(cs) > 0);

    /* Day names */
    assert(strcmp(nl_langinfo(DAY_1), "Sunday") == 0);
    assert(strcmp(nl_langinfo(DAY_2), "Monday") == 0);
    assert(strcmp(nl_langinfo(DAY_3), "Tuesday") == 0);
    assert(strcmp(nl_langinfo(DAY_4), "Wednesday") == 0);
    assert(strcmp(nl_langinfo(DAY_5), "Thursday") == 0);
    assert(strcmp(nl_langinfo(DAY_6), "Friday") == 0);
    assert(strcmp(nl_langinfo(DAY_7), "Saturday") == 0);

    /* Abbreviated days */
    assert(strcmp(nl_langinfo(ABDAY_1), "Sun") == 0);
    assert(strcmp(nl_langinfo(ABDAY_2), "Mon") == 0);

    /* Month names */
    assert(strcmp(nl_langinfo(MON_1), "January") == 0);
    assert(strcmp(nl_langinfo(MON_12), "December") == 0);
    assert(strcmp(nl_langinfo(ABMON_1), "Jan") == 0);
    assert(strcmp(nl_langinfo(ABMON_12), "Dec") == 0);

    /* AM/PM */
    assert(strcmp(nl_langinfo(AM_STR), "AM") == 0);
    assert(strcmp(nl_langinfo(PM_STR), "PM") == 0);

    /* Radix (decimal point) */
    assert(strcmp(nl_langinfo(RADIXCHAR), ".") == 0);

    /* Thousands separator (empty in C) */
    assert(strcmp(nl_langinfo(THOUSEP), "") == 0);

    /* Yes/No expressions */
    assert(strlen(nl_langinfo(YESEXPR)) > 0);
    assert(strlen(nl_langinfo(NOEXPR)) > 0);

    /* ===== en_US.UTF-8 locale ===== */

    loc = setlocale(LC_ALL, "en_US.UTF-8");
    if (loc != NULL) {
        /* LC_CTYPE */
        assert(strcmp(nl_langinfo(CODESET), "UTF-8") == 0);

        /* LC_NUMERIC */
        assert(strcmp(nl_langinfo(RADIXCHAR), ".") == 0);
        assert(strcmp(nl_langinfo(THOUSEP), ",") == 0);

        /* LC_MONETARY */
        lc = localeconv();
        assert(lc != NULL);
        assert(strcmp(lc->currency_symbol, "$") == 0);
        assert(strcmp(lc->mon_decimal_point, ".") == 0);
        assert(strcmp(lc->mon_thousands_sep, ",") == 0);
        assert(strcmp(lc->int_curr_symbol, "USD ") == 0);
        assert(lc->frac_digits == 2);
        assert(lc->int_frac_digits == 2);

        /* LC_TIME — day/month names same as C for en_US */
        assert(strcmp(nl_langinfo(DAY_1), "Sunday") == 0);
        assert(strcmp(nl_langinfo(MON_1), "January") == 0);

        /* LC_NUMERIC via localeconv */
        assert(strcmp(lc->decimal_point, ".") == 0);
        assert(strcmp(lc->thousands_sep, ",") == 0);

        /* Per-category setlocale */
        loc = setlocale(LC_CTYPE, "en_US.UTF-8");
        assert(loc != NULL);
        loc = setlocale(LC_NUMERIC, "en_US.UTF-8");
        assert(loc != NULL);
        loc = setlocale(LC_MONETARY, "en_US.UTF-8");
        assert(loc != NULL);

        /* Restore C locale */
        setlocale(LC_ALL, "C");

        /* Verify C locale is fully restored */
        assert(strcmp(nl_langinfo(CODESET), "ANSI_X3.4-1968") == 0);
        assert(strcmp(nl_langinfo(THOUSEP), "") == 0);
        lc = localeconv();
        assert(strcmp(lc->currency_symbol, "") == 0);
        assert(strcmp(lc->thousands_sep, "") == 0);
        assert(lc->frac_digits == 127);
    }

    /* ===== ctype (C locale) ===== */

    assert(toupper('a') == 'A');
    assert(toupper('z') == 'Z');
    assert(toupper('A') == 'A');
    assert(toupper('5') == '5');
    assert(tolower('Z') == 'z');
    assert(tolower('a') == 'a');

    assert(isblank(' '));
    assert(isblank('\t'));
    assert(!isblank('\n'));
    assert(!isblank('a'));

    assert(isalnum('0'));
    assert(isalnum('9'));
    assert(isalnum('a'));
    assert(isalnum('Z'));
    assert(!isalnum(' '));
    assert(!isalnum('\0'));

    assert(isprint(' '));
    assert(!isprint('\x01'));
    assert(iscntrl('\x01'));
    assert(iscntrl('\x7f'));
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

    assert(strftime(buf, sizeof(buf), "%A", &t) > 0);
    assert(strcmp(buf, "Monday") == 0);
    assert(strftime(buf, sizeof(buf), "%a", &t) > 0);
    assert(strcmp(buf, "Mon") == 0);
    assert(strftime(buf, sizeof(buf), "%B", &t) > 0);
    assert(strcmp(buf, "January") == 0);
    assert(strftime(buf, sizeof(buf), "%b", &t) > 0);
    assert(strcmp(buf, "Jan") == 0);
    assert(strftime(buf, sizeof(buf), "%Y", &t) > 0);
    assert(strcmp(buf, "2024") == 0);
    assert(strftime(buf, sizeof(buf), "%Y-%m-%d", &t) > 0);
    assert(strcmp(buf, "2024-01-15") == 0);
    assert(strftime(buf, sizeof(buf), "%H:%M:%S", &t) > 0);
    assert(strcmp(buf, "14:30:45") == 0);
    assert(strftime(buf, sizeof(buf), "%p", &t) > 0);
    assert(strcmp(buf, "PM") == 0);

    /* ===== gmtime / localtime / mktime ===== */

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

    /* ===== tzdata.zi availability ===== */

    /* Verify the timezone data file is accessible in lindfs.
       This is needed by C++20 std::chrono::get_tzdb(). */
    assert(access("/usr/share/zoneinfo/tzdata.zi", R_OK) == 0);
    assert(access("/usr/share/zoneinfo/leap-seconds.list", R_OK) == 0);

    /* /etc/timezone should exist for current_zone() fallback */
    assert(access("/etc/timezone", R_OK) == 0);

    /* ===== DST transition via POSIX TZ string ===== */

    /* US Eastern: EST5EDT,M3.2.0,M11.1.0
       DST starts 2nd Sunday of March, ends 1st Sunday of November */
    setenv("TZ", "EST5EDT,M3.2.0,M11.1.0", 1);
    tzset();

    /* Jan 15 2024 12:00 UTC — should be EST (UTC-5) */
    time_t jan = 1705320000;  /* 2024-01-15 12:00:00 UTC */
    lt = localtime(&jan);
    assert(lt != NULL);
    assert(lt->tm_hour == 7);   /* 07:00 EST */
    assert(lt->tm_isdst == 0);

    /* Jul 15 2024 12:00 UTC — should be EDT (UTC-4) */
    time_t jul = 1721044800;  /* 2024-07-15 12:00:00 UTC */
    lt = localtime(&jul);
    assert(lt != NULL);
    assert(lt->tm_hour == 8);   /* 08:00 EDT */
    assert(lt->tm_isdst == 1);

    /* ===== strftime with en_US.UTF-8 locale ===== */

    if (setlocale(LC_ALL, "en_US.UTF-8") != NULL) {
        setenv("TZ", "UTC0", 1);
        tzset();

        /* strftime %c (locale-dependent date/time) should not crash */
        t.tm_year = 124;
        t.tm_mon = 6;       /* July */
        t.tm_mday = 4;
        t.tm_hour = 10;
        t.tm_min = 0;
        t.tm_sec = 0;
        t.tm_wday = 4;      /* Thursday */
        t.tm_yday = 185;
        assert(strftime(buf, sizeof(buf), "%c", &t) > 0);
        assert(strlen(buf) > 0);

        /* %x (locale date) and %X (locale time) */
        assert(strftime(buf, sizeof(buf), "%x", &t) > 0);
        assert(strlen(buf) > 0);
        assert(strftime(buf, sizeof(buf), "%X", &t) > 0);
        assert(strlen(buf) > 0);

        setlocale(LC_ALL, "C");
    }

    write(1, "done\n", 5);
    return 0;
}
