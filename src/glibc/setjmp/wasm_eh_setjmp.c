/*
 * Wasm EH-based setjmp/longjmp runtime support for lind-wasm.
 *
 * When user code is compiled with -fwasm-exceptions -mllvm -wasm-enable-sjlj,
 * clang 18 transforms each setjmp()/longjmp() call site into:
 *
 *   setjmp(buf):
 *     table = saveSetjmp(buf, label_id, table, size);
 *     size  = getTempRet0();
 *     try { ... } catch (__c_longjmp) { ... testSetjmp(buf[0], table, size) ... }
 *
 *   longjmp(buf, val):
 *     __wasm_longjmp(buf, val)   →   throw __c_longjmp(&buf[2])
 *
 * jmp_buf layout (as used by this implementation):
 *   buf[0]  registration token set by saveSetjmp (used by testSetjmp)
 *   buf[1]  unused (reserved)
 *   buf[2]  self-reference set by __wasm_longjmp (payload for throw)
 *   buf[3]  normalised return value set by __wasm_longjmp
 *
 * The jmp_buf typedef is long int[8] (32 bytes on wasm32), which is
 * large enough for all four 4-byte slots used here.
 *
 * buf[0] holds a per-call unique token rather than buf's own address.  This
 * matters for jmp_bufs that live in a fixed-address global and get saved and
 * restored as raw bytes by their caller (a common unwind-protect idiom built
 * on setjmp/longjmp: snapshot sizeof(jmp_buf), memcpy it back later to
 * restore an outer context): using buf's address as the token would make
 * every registration on that global indistinguishable, since the address
 * never changes.  A fresh token per saveSetjmp() call means a caller that
 * restores an *older* snapshot of the buffer (reverting buf[0] to an outer,
 * still-live registration's token) correctly falls through this frame's
 * table and resolves in whichever outer frame's table still holds that
 * token — matching what raw stack/register snapshot restoration does
 * natively.  See testSetjmp() for the failure mode this avoids.
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <lind_debug.h>


/* Thread-local "second return value" communicated between saveSetjmp and its
 * caller via getTempRet0 (wasm has no multi-value imported functions yet). */
static __thread int g_tempRet0;

/* Thread-local monotonic counter used to mint a unique, never-zero token for
 * each saveSetjmp() registration (see buf[0] discussion above). */
static __thread int g_setjmp_token;

__attribute__((visibility("default"))) void setTempRet0(int val)
{
    g_tempRet0 = val;
}

__attribute__((visibility("default"))) int getTempRet0(void)
{
    return g_tempRet0;
}

/*
 * saveSetjmp(buf, label_id, table, size) → new_table
 *
 * Registers the (token, label_id) pair in *table* and stores a fresh,
 * never-zero token in buf[0] so testSetjmp can identify the frame later
 * (see the file header comment for why this is a per-call token rather than
 * buf's own address).  Grows the table (via realloc) when full.
 * Communicates the (possibly new) table size via setTempRet0().
 *
 * The initial table is allocated by the caller (malloc(40), size=4).
 */
__attribute__((visibility("default"))) int *saveSetjmp(int *buf, int label_id, int *table, int size)
{
    int token = ++g_setjmp_token;
    if (token == 0) token = ++g_setjmp_token; /* skip the 0 sentinel on wraparound */
    buf[0] = token;

    /* Find an empty slot. */
    for (int i = 0; i < size; i++) {
        if (table[2 * i] == 0) {
            table[2 * i]     = token;
            table[2 * i + 1] = label_id;
            setTempRet0(size);
            return table;
        }
    }

    /* Table full: double it. */
    int new_size = size * 2;
    int *new_table = (int *)realloc(table, (size_t)new_size * 2 * sizeof(int));
    if (!new_table) {
        lind_debug_panic("saveSetjmp: OOM growing setjmp table");
        setTempRet0(size);
        return table;
    }

    /* Zero the freshly allocated half. */
    memset(new_table + size * 2, 0, (size_t)size * 2 * sizeof(int));

    new_table[size * 2]     = token;
    new_table[size * 2 + 1] = label_id;

    setTempRet0(new_size);
    return new_table;
}

/*
 * testSetjmp(env, table, size) → label_id or 0
 *
 * Called inside the EH catch block.  Searches *table* for a registration
 * whose token matches *env* (== buf[0], read fresh from the jmp_buf at catch
 * time).  Returns the label_id of the matching setjmp frame, or 0 if not
 * found (meaning the exception belongs to an outer frame and must be
 * re-thrown).
 *
 * *table* is shared by every setjmp() call site within one function
 * invocation (see saveSetjmp), so it can hold more than one entry: e.g. when
 * a function calls setjmp() on the same jmp_buf more than once (a loop, or
 * two call sites merged into one function by inlining), each call appends a
 * new entry with a fresh token rather than replacing the old one.  Because
 * tokens are unique per call, at most one entry can ever match a given env
 * value, so search order doesn't matter for correctness; newest-first is
 * kept simply because the live registration is usually the most recently
 * appended one, making the common case a single-iteration scan.
 *
 * Entries are intentionally left in the table after a match (not cleared).
 * buf[0]/env reflects whatever a caller's raw jmp_buf save/restore (e.g. an
 * unwind-protect helper built on memcpy(&saved, &buf, sizeof(buf))) currently
 * holds, which may be an older, still-live outer registration's token rather
 * than this frame's own — see the file header comment.  In that case env
 * simply won't match anything in *this* table and correctly falls through to
 * be re-thrown to the enclosing frame, which still holds that token in its
 * own table.  This is what actually resolves an unbounded catch/rethrow loop
 * that used to occur when buf[0] was the jmp_buf's own (fixed,
 * restore-invariant) address instead of a token: a caller's post-catch
 * re-throw to the same buffer kept re-matching this frame's own entry
 * forever, exhausting the null-collected GC heap one exception object at a
 * time until it crashed with "allocation size too large" instead of
 * terminating.
 */
__attribute__((visibility("default"))) int testSetjmp(int env, int *table, int size)
{
    for (int i = size - 1; i >= 0; i--) {
        if (table[2 * i] == env) {
            return table[2 * i + 1];
        }
    }
    return 0;
}

/*
 * __wasm_longjmp(buf, val)
 *
 * Stores the normalised return value in buf[3] and a self-pointer in buf[2],
 * then throws the __c_longjmp wasm exception with &buf[2] as the i32 payload.
 *
 * __builtin_wasm_throw(1, ...) selects the __c_longjmp tag (index 1 in LLVM's
 * wasm EH/sjlj ABI; index 0 is __cpp_exception).  wasm-ld relocates the throw
 * instruction symbolically, so the final index in the linked binary is correct.
 */
__attribute__((visibility("default"), noreturn)) void __wasm_longjmp(int *buf, int val)
{
    buf[3] = val != 0 ? val : 1;
    buf[2] = (int)(uintptr_t)buf;
    __builtin_wasm_throw(1, (void *)(buf + 2));
}
