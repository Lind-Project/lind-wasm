/*
 * Shared helpers for the correctness tests under cow-tests/correctness
 *
 * These tests validate the OBSERVABLE behavior fork() must preserve
 * regardless of whether it's implemented via eager copy (today) or
 * COW (see cow-design.md, cow-implementation-plan.md at repo root). None
 * of these tests can observe whether a copy happened internally -- that's an
 * implementation detail behind fork()'s POSIX semantics. What they check
 * is that the guest-visible data isolation/inheritance invariants hold,
 * using the same fill/verify pattern from independent processes so no
 * shared reference buffer or IPC is needed to know what "correct" means.
 */
#ifndef COW_TEST_H
#define COW_TEST_H

#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

/* Deterministic, seed-dependent byte generator. Any process that knows
 * (offset, seed) can independently recompute the expected byte without
 * needing to compare against a buffer owned by a different cage. */
static inline unsigned char cow_pattern_byte(size_t index, uint32_t seed) {
    uint32_t x = (uint32_t)(index * 2654435761u) + seed;
    x ^= x >> 13;
    x *= 0x85ebca6bu;
    x ^= x >> 16;
    return (unsigned char)x;
}

static inline void cow_fill(unsigned char *buf, size_t len, uint32_t seed) {
    for (size_t i = 0; i < len; i++) {
        buf[i] = cow_pattern_byte(i, seed);
    }
}

/* Fill a sub-range [start, start+len) of a logically larger buffer. Always
 * pass the buffer's true, unoffset base pointer -- this indexes base[start+i]
 * itself -- so a partial-range fill/verify is trivially consistent with a
 * full-range verify over the same seed without the caller having to keep a
 * pre-offset pointer and a matching `start` in sync by hand. */
static inline void cow_fill_range(unsigned char *base, size_t start, size_t len, uint32_t seed) {
    for (size_t i = 0; i < len; i++) {
        base[start + i] = cow_pattern_byte(start + i, seed);
    }
}

/* Returns 1 if buf[0..len) matches the pattern for `seed`, 0 otherwise.
 * Prints the first mismatch to stderr (prefixed with `who`) for triage. */
static inline int cow_verify(const unsigned char *buf, size_t len, uint32_t seed, const char *who) {
    for (size_t i = 0; i < len; i++) {
        unsigned char expect = cow_pattern_byte(i, seed);
        if (buf[i] != expect) {
            fprintf(stderr, "%s: mismatch at offset %zu: expected 0x%02x got 0x%02x\n",
                    who, i, expect, buf[i]);
            return 0;
        }
    }
    return 1;
}

/* Same base-pointer convention as cow_fill_range: pass the true, unoffset
 * base pointer; this checks base[start+i]. */
static inline int cow_verify_range(const unsigned char *base, size_t start, size_t len, uint32_t seed, const char *who) {
    for (size_t i = 0; i < len; i++) {
        unsigned char expect = cow_pattern_byte(start + i, seed);
        if (base[start + i] != expect) {
            fprintf(stderr, "%s: mismatch at rel-offset %zu (abs %zu): expected 0x%02x got 0x%02x\n",
                    who, i, start + i, expect, base[start + i]);
            return 0;
        }
    }
    return 1;
}

static inline int cow_all_zero(const unsigned char *buf, size_t len, const char *who) {
    for (size_t i = 0; i < len; i++) {
        if (buf[i] != 0) {
            fprintf(stderr, "%s: expected zero at offset %zu, got 0x%02x\n", who, i, buf[i]);
            return 0;
        }
    }
    return 1;
}

/* Every correctness test uses exit(1)/_exit(1) with a stderr message on
 * failure (caught by the runner as a nonzero exit code + native/wasm
 * comparison) and falls through to return 0 on success, matching the
 * convention used by tests/unit-tests/process_tests/deterministic. */
#define COW_FAIL(msg) do { fprintf(stderr, "FAIL(%s:%d): %s\n", __FILE__, __LINE__, msg); _exit(1); } while (0)
#define COW_CHECK(cond, msg) do { if (!(cond)) COW_FAIL(msg); } while (0)

#endif /* COW_TEST_H */
