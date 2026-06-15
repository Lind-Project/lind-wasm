// Measurement cage: exercises libz functions through the auto-interposition grate.
// Marshalled (interposed → grate): adler32, crc32, compressBound, compress, uncompress.
// force_local (runs locally):       zlibVersion.
// Prints PASS/FAIL per function so we can tally what the auto-marshalling handles.
#include <stdio.h>
#include <string.h>

extern unsigned long adler32(unsigned long, const unsigned char *, unsigned int);
extern unsigned long crc32(unsigned long, const unsigned char *, unsigned int);
extern unsigned long compressBound(unsigned long);
extern int compress(unsigned char *dest, unsigned long *destLen,
                    const unsigned char *source, unsigned long sourceLen);
extern int uncompress(unsigned char *dest, unsigned long *destLen,
                      const unsigned char *source, unsigned long sourceLen);
extern const char *zlibVersion(void);

static int pass = 0, fail = 0;
static void check(const char *name, int ok, const char *detail) {
    if (ok) { printf("  PASS  %s  %s\n", name, detail); pass++; }
    else    { printf("  FAIL  %s  %s\n", name, detail); fail++; }
}

// known data in the data segment (cross-cage reachable)
static const unsigned char wiki[] = "Wikipedia";
static const unsigned char nums[] = "123456789";
static const unsigned char payload[] =
    "the quick brown fox jumps over the lazy dog; "
    "the quick brown fox jumps over the lazy dog.";

int main(void) {
    char buf[64];

    // 1. adler32 (marshalled) — vector: adler32(1,"Wikipedia",9) == 0x11E60398
    unsigned long a = adler32(1, wiki, 9);
    snprintf(buf, sizeof buf, "= 0x%lx (want 0x11e60398)", a);
    check("adler32", a == 0x11E60398UL, buf);

    // 2. crc32 (marshalled) — vector: crc32(0,"123456789",9) == 0xCBF43926
    unsigned long c = crc32(0, nums, 9);
    snprintf(buf, sizeof buf, "= 0x%lx (want 0xcbf43926)", c);
    check("crc32", c == 0xCBF43926UL, buf);

    // 3. compressBound (marshalled) — should exceed input length
    unsigned long bound = compressBound(sizeof(payload));
    snprintf(buf, sizeof buf, "= %lu (want > %lu)", bound, (unsigned long)sizeof(payload));
    check("compressBound", bound > sizeof(payload), buf);

    // 4. compress + uncompress round-trip (both marshalled)
    unsigned char comp[256];
    unsigned long comp_len = sizeof(comp);
    int rc = compress(comp, &comp_len, payload, sizeof(payload));
    if (rc != 0) {
        snprintf(buf, sizeof buf, "compress rc=%d", rc);
        check("compress", 0, buf);
    } else {
        snprintf(buf, sizeof buf, "rc=0 comp_len=%lu", comp_len);
        check("compress", comp_len > 0 && comp_len < sizeof(comp), buf);

        unsigned char back[256];
        unsigned long back_len = sizeof(back);
        int ru = uncompress(back, &back_len, comp, comp_len);
        int ok = (ru == 0) && (back_len == sizeof(payload))
                 && (memcmp(back, payload, sizeof(payload)) == 0);
        snprintf(buf, sizeof buf, "ru=%d back_len=%lu (want %lu, roundtrip)",
                 ru, back_len, (unsigned long)sizeof(payload));
        check("uncompress", ok, buf);
    }

    // 5. zlibVersion (force_local) — runs locally, should return a version string
    const char *v = zlibVersion();
    check("zlibVersion(force_local)", v != 0 && v[0] != 0, v ? v : "(null)");

    printf("[libz-app] %d passed, %d failed (of marshalled+local libz calls)\n", pass, fail);
    return fail == 0 ? 0 : 1;
}
