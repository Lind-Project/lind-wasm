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
 *   buf[0]  self-reference set by saveSetjmp (used by testSetjmp)
 *   buf[1]  unused (reserved)
 *   buf[2]  self-reference set by __wasm_longjmp (payload for throw)
 *   buf[3]  normalised return value set by __wasm_longjmp
 *
 * The jmp_buf typedef is long int[8] (32 bytes on wasm32), which is
 * large enough for all four 4-byte slots used here.
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <lind_debug.h>


/* Thread-local "second return value" communicated between saveSetjmp and its
 * caller via getTempRet0 (wasm has no multi-value imported functions yet). */
static __thread int g_tempRet0;

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
 * Registers the (buf, label_id) pair in *table* and stores buf's own address
 * in buf[0] so testSetjmp can identify the frame later.  Grows the table
 * (via realloc) when full.  Communicates the (possibly new) table size via
 * setTempRet0().
 *
 * The initial table is allocated by the caller (malloc(40), size=4).
 */
__attribute__((visibility("default"))) int *saveSetjmp(int *buf, int label_id, int *table, int size)
{
    /* Store self-reference so testSetjmp can match on buf[0]. */
    buf[0] = (int)(uintptr_t)buf;

    /* Find an empty slot. */
    for (int i = 0; i < size; i++) {
        if (table[2 * i] == 0) {
            table[2 * i]     = (int)(uintptr_t)buf;
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

    new_table[size * 2]     = (int)(uintptr_t)buf;
    new_table[size * 2 + 1] = label_id;

    setTempRet0(new_size);
    return new_table;
}

/*
 * testSetjmp(env, table, size) → label_id or 0
 *
 * Called inside the EH catch block.  Searches *table* for a buf whose
 * address matches *env* (== buf[0] set by saveSetjmp).  Returns the
 * label_id of the matching setjmp frame, or 0 if not found (meaning the
 * exception belongs to an outer frame and must be re-thrown).
 */
__attribute__((visibility("default"))) int testSetjmp(int env, int *table, int size)
{
    for (int i = 0; i < size; i++) {
        if (table[2 * i] == env)
            return table[2 * i + 1];
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
