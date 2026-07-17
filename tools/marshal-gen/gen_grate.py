#!/usr/bin/env python3
"""Generate an auto-interposition grate from an inference marshalling spec.

Input:  <lib>.marshal.json  (from the marshal-infer inference tool)
Output: <lib>_auto_grate.c  (a grate that registers a generic marshalling
        handler for every `decision:"marshal"` function, bound to (spec, &real_fn))

The generated grate:
  * compiles static, with --compile-grate --fpcast-emu, statically linked against
    the library's .a so the real functions are DEFINED in the grate cage;
  * uses ONE generic dispatcher (the registered value is a `struct libctx*`
    carrying {spec, &real_fn}); lind_marshal_dispatch marshals per spec then
    calls the real function through the uniform fpcast pointer;
  * leaves `decision:"force_local"` (and unlisted) functions un-interposed —
    they run against the app's own preloaded library.

Usage:
  gen_grate.py <lib>.marshal.json --lib-name libz --out libz_auto_grate.c
"""
import argparse
import json
import sys

# --- JSON vocab -> lind_marshal.h enum mapping ---
ARG_KIND = {"scalar": "LIND_ARG_SCALAR", "ptr": "LIND_ARG_PTR", "handle": "LIND_ARG_HANDLE"}
DIR = {"in": "LIND_PTR_IN", "out": "LIND_PTR_OUT", "inout": "LIND_PTR_INOUT"}
SIZE_KIND = {
    "const": "LIND_SIZE_CONST",
    "from_arg": "LIND_SIZE_FROM_ARG",
    "from_arg_pointee": "LIND_SIZE_FROM_ARG_POINTEE",
    "cstr": "LIND_SIZE_CSTR",
    "ptr_array": "LIND_SIZE_PTR_ARRAY",
}
RET_KIND = {
    "void": "LIND_RET_VOID",
    "scalar": "LIND_RET_SCALAR",
    "ptr_alias_arg": "LIND_RET_PTR_ALIAS_ARG",
    "ptr_into_arg": "LIND_RET_PTR_INTO_ARG",
    "handle": "LIND_RET_HANDLE",
}

# Features the runtime can faithfully marshal. A function whose spec uses anything
# outside these is dropped to force_local (run un-interposed) rather than silently
# mis-marshalled — e.g. ptr_alloc/ptr_to_static/ptr_into_cursor returns would hand
# the app a grate-cage pointer it can't dereference.
SUPPORTED_RET = {"void", "scalar", "ptr_alias_arg", "ptr_into_arg", "handle"}
SUPPORTED_SIZE = {None, "none", "na", "const", "from_arg", "from_arg_pointee",
                  "cstr", "ptr_array"}


# Name substrings whose functions cannot be interposed in a static grate:
#  - setjmp/longjmp: toolchain rejects taking setjmp's address; longjmp restores
#    a jmp_buf saved on the *caller* cage's stack, so it must run there
#  - locale: static-linking drags in unresolved TLS/locale machinery
#  - printf/scanf: variadic — the spec can't describe `...` args, so marshalling
#    them corrupts the call (and the inference wrongly marks them marshal)
# NOTE: "exit"/"exec" are deliberately NOT here as substrings — both are broad
# enough to false-positive on unrelated functions (__cyg_profile_func_exit is an
# instrumentation hook, not a terminator; re_exec is a legacy regex matcher, not
# process exec). The real exit-/exec-family members are listed by exact name in
# NEVER_INTERPOSE below instead.
NEVER_INTERPOSE_SUBSTR = (
    "setjmp", "longjmp", "locale", "printf", "scanf",
)
# Other variadic / address-unsafe / terminating functions to exclude by exact name.
NEVER_INTERPOSE = {
    # control-flow terminators: interposed, they run in the grate cage and RETURN
    # to the app, so the app's exit() never ends the app cage — glibc's `_exit`
    # (`while(1){exit_group();exit();}`) then spins forever. Must be force_local.
    "exit", "_exit", "_Exit", "quick_exit", "pthread_exit", "abort",
    # image replacement: replaces the *calling* cage's image; only meaningful there.
    "execl", "execlp", "execle", "execv", "execve", "execveat", "execvp",
    "execvpe", "fexecve",
    "fcntl", "ioctl", "open", "open64",
    "syscall", "err", "errx", "warn", "warnx", "verr", "verrx", "vwarn", "vwarnx",
    "syslog", "vsyslog", "prctl", "ptrace", "semctl",
    # DIAGNOSTIC (pending discussion): per-cage runtime *init* functions called by
    # __libc_start_main during startup. They set up the calling cage's TLS / ctype
    # tables / thread pointer; interposed, they initialize the *grate* cage instead,
    # leaving the app's runtime state uninitialized (e.g. breaks whole-number %f).
    "__libc_setup_tls", "__ctype_init", "__wasi_init_tp",
    # STATIC-LINK-BLOCKED on this port (not a marshalling issue): a static grate must
    # resolve every symbol libm.a's *implementation objects* transitively reference,
    # not just what we call directly. These pull in ceill/floorl/rintl/truncl (long
    # double gamma helpers) or __ieee754_{acosl,fmodl,remainder} (double remainder's
    # own internal impl symbol), which are genuinely missing from this build's libm —
    # see LIBM_INTERPOSITION.md. Confirmed via a static-link attempt with full errors
    # (llvm-nm on the blocking .o members) — includes the f64x/f32x weak aliases that
    # resolve to the same long-double-backed implementation objects (ldbl-96 makes
    # float64x/etc wider than double, so they alias the *l long-double body, not the
    # plain double one).
    "acosl", "acosf64x", "fmodl", "fmodf64x",
    "remainder", "drem", "remainderf32x", "remainderf64",
    "gammal", "lgammal", "lgammal_r", "tgammal",
    "lgammaf64x", "lgammaf64x_r", "tgammaf64x",
    "powl", "powf64x", "remquol", "remquof64x",
    # BINARYEN wasm-opt bug (not a marshalling issue): triggers a parser crash
    # ("popping from empty stack") in the fpcast-emu / epoch-injection pass, both
    # standalone (see LIBM_INTERPOSITION.md's `test-double-scalb` opt-failure) and
    # when linked into this grate. `scalbln` (long-exponent variant) is unaffected.
    "scalb", "scalbf", "scalbl",
}

# Functions that operate on, or hand out, a file descriptor. A POSIX fd is a
# per-cage handle (an index into *that* cage's fdtable), passed/returned as a bare
# int the marshaller can't distinguish from any other int. Interposed, the call
# runs in the grate cage: an fd the app opened (open() is force_local) is invalid
# there → EBADF, and an fd the grate creates (socket/pipe/...) is unusable by the
# app. So the whole fd family must be force_local — same reason open/fcntl/ioctl
# already are. (Proper long-term fix: teach marshal-infer to flag fd params/returns
# so this is data-driven instead of a name list.)
FD_FUNCS = {
    # operate on an fd (first arg)
    "read", "write", "close", "lseek", "lseek64", "pread", "pwrite", "pread64",
    "pwrite64", "readv", "writev", "preadv", "pwritev", "preadv64", "pwritev64",
    "preadv2", "pwritev2", "dup", "dup2", "dup3", "fchdir", "fchmod", "fchown",
    "fstat", "fstat64", "fstatfs", "fstatfs64", "fstatvfs", "fstatvfs64", "fsync",
    "fdatasync", "ftruncate", "ftruncate64", "flock", "fcntl64", "sendfile",
    "sendfile64", "fgetxattr", "fsetxattr", "flistxattr", "fremovexattr", "getdents",
    "getdents64", "epoll_ctl", "epoll_wait", "epoll_pwait", "epoll_pwait2",
    "fpathconf", "isatty", "ttyname", "ttyname_r", "tcgetattr", "tcsetattr",
    "tcflush", "tcdrain", "tcflow", "tcsendbreak", "tcgetsid", "tcgetpgrp",
    "tcsetpgrp", "fdopendir", "fdopen", "syncfs", "posix_fadvise", "posix_fadvise64",
    "posix_fallocate", "posix_fallocate64", "fallocate", "readahead", "flockfile",
    # socket fd (first arg)
    "accept", "accept4", "bind", "listen", "connect", "send", "recv", "sendto",
    "recvfrom", "sendmsg", "recvmsg", "recvmmsg", "sendmmsg", "getsockopt",
    "setsockopt", "getsockname", "getpeername", "shutdown", "sockatmark",
    # create / return an fd
    "socket", "socketpair", "pipe", "pipe2", "eventfd", "eventfd2", "epoll_create",
    "epoll_create1", "timerfd_create", "timerfd_settime", "timerfd_gettime",
    "signalfd", "inotify_init", "inotify_init1", "inotify_add_watch",
    "inotify_rm_watch", "memfd_create", "creat", "creat64", "mkstemp", "mkstemp64",
    "mkostemp", "mkostemp64",
    # *at family: first arg is a dirfd (or AT_FDCWD, itself cage-relative)
    "openat", "openat64", "faccessat", "faccessat2", "fchmodat", "fchownat", "fstatat",
    "fstatat64", "newfstatat", "linkat", "mkdirat", "mknodat", "readlinkat",
    "renameat", "renameat2", "symlinkat", "unlinkat", "utimensat", "statx",
    "name_to_handle_at", "open_by_handle_at",
}


def is_marshalable(f):
    """True iff the runtime can faithfully marshal every part of this spec."""
    name = f.get("name", "")
    if name in NEVER_INTERPOSE or name in FD_FUNCS \
            or any(s in name for s in NEVER_INTERPOSE_SUBSTR):
        return False
    ret = f.get("ret") or {}
    r = ret.get("kind")
    if r is not None and r not in SUPPORTED_RET:
        return False  # ptr_alloc, ptr_to_static, ptr_into_cursor, ...
    # NOTE: no `type == "complex"` exclusion anymore. marshal-infer now detects
    # byval/sret-lowered arguments and returns (C99 _Complex, ordinary large
    # by-value structs, and long double's sret-shaped return) via LLVM IR
    # attributes / a hardcoded fp128 rule and represents them as ordinary
    # `kind:"ptr"` entries (const-sized, IN for byval args with no copy-back,
    # OUT for the synthetic leading sret pointer) -- the SAME shape the
    # existing LIND_ARG_PTR path below already handles for any other
    # pointer-taking function, so no extra check is needed here. `"type"` is
    # purely an informational label on the pointee now, not a kind that
    # affects marshalling. See issues/fix-complex-and-ldbl-abi-marshalling.md.
    # (long double's ARGUMENT side is still unfixed -- it splits into 2 raw
    # wasm slots invisible at marshal-infer's IR level, not byval-lowered, so
    # the function stays force_local upstream and never reaches this check as
    # "marshal" at all.)

    def walk(n):
        if n.get("cursor"):                       # strsep-style cursor: not implemented
            return False
        if n.get("kind") == "ptr" and n.get("size_kind") not in SUPPORTED_SIZE:
            return False                          # unknown sizing -> can't copy safely
        for ch in (n.get("pointee") or []) + (n.get("fields") or []):
            if not walk(ch):
                return False
        return True

    return all(walk(a) for a in f.get("args", []))

# C identifiers that are valid function names but need extern decls with the
# right signature would be ideal; we use a generic extern returning long and
# taking ints. fpcast-emu adapts the ABI, so the exact prototype only needs to
# be *callable* (address-taken). We emit `extern <ret> name();` (K&R, no
# prototype) so any call/address-of compiles.

class Emitter:
    def __init__(self):
        self.decls = []      # pre-spec declarations (layouts, fields)
        self._n = 0

    def uid(self, base):
        self._n += 1
        return f"{base}_{self._n}"

    def arg_spec_body(self, a):
        """Return the C initializer body (inside braces) for one lind_arg_spec,
        emitting any nested layout declarations into self.decls first."""
        kind = a.get("kind", "scalar")
        if kind == "scalar":
            return ".kind = LIND_ARG_SCALAR"
        if kind == "handle":
            cls = a.get("handle_class", "void")
            return f'.kind = LIND_ARG_HANDLE, .handle_class = {self._handle_class_id(cls)}'
        if kind != "ptr":
            # Fallback: treat unknown as scalar passthrough.
            return ".kind = LIND_ARG_SCALAR"

        parts = [".kind = LIND_ARG_PTR"]
        d = a.get("dir", "in")
        parts.append(f".ptr_direction = {DIR.get(d, 'LIND_PTR_IN')}")
        sk = a.get("size_kind", "const")
        parts.append(f".size_kind = {SIZE_KIND.get(sk, 'LIND_SIZE_NONE')}")
        if sk == "const":
            parts.append(f'.const_size = {a.get("const_size", 0)}')
        if sk in ("from_arg", "from_arg_pointee"):
            idx = a.get("size_arg_index", a.get("size_field_index", 0))
            parts.append(f".size_arg_index = {idx}")

        # NULL-terminated array of pointers (argv): emit the per-element spec.
        if sk == "ptr_array":
            pointee = a.get("pointee") or []
            if pointee:
                elem_name = self.emit_element(pointee[0])
                parts.append(f".element = &{elem_name}")
            return ", ".join(parts)

        # Struct pointee -> emit a lind_layout and reference it. (Unions are NOT
        # field-chased — without a discriminator we can't know the active arm, so a
        # union pointee is left as a flat const-sized blit, which is safe for the
        # opaque-bytes unions seen in pthread/libc internals.)
        pointee = a.get("pointee") or []
        if pointee and pointee[0].get("kind") == "struct":
            layout_name = self.emit_layout(pointee[0])
            parts.append(f".layout = &{layout_name}")
        # OUT pointer-to-pointer aliasing an arg (strtol's char** endptr): the
        # pointee is a `ptr_into_arg` node — the written inner pointer points into
        # arg `into_arg`. Encode 1-based (0 = none) so the default stays "not applicable".
        elif pointee and pointee[0].get("kind") == "ptr_into_arg":
            into = pointee[0].get("into_arg", 0)
            parts.append(f".out_ptr_into_arg1 = {into + 1}")
        return ", ".join(parts)

    def _handle_class_id(self, cls):
        # lind_marshal.h handle_class is a uint32_t; we hash the class string to
        # a stable small int. For libz there are no handles in the marshalable
        # set, so this is rarely hit.
        return f"{(hash(cls) & 0x7fffffff)}u /* {cls} */"

    def emit_element(self, node):
        """Emit a standalone lind_arg_spec for an array element; return its name."""
        body = self.arg_spec_body(node)  # may append nested decls first
        name = self.uid("elem")
        self.decls.append(f"static struct lind_arg_spec {name} = {{ {body} }};")
        return name

    def emit_layout(self, st):
        """Emit lind_field[] + lind_layout for a struct node; return layout name."""
        fields = st.get("fields") or []
        field_inits = []
        for f in fields:
            spec_name = self.uid("fspec")
            self.decls.append(
                f"static struct lind_arg_spec {spec_name} = {{ {self.arg_spec_body(f)} }};"
            )
            touched = 1 if f.get("touched") else 0
            field_inits.append(
                f'    {{ .offset = {f.get("offset", 0)}, .spec = &{spec_name}, .touched = {touched} }},'
            )
        fields_name = self.uid("fields")
        self.decls.append(
            f"static struct lind_field {fields_name}[] = {{\n" + "\n".join(field_inits) + "\n};"
        )
        layout_name = self.uid("layout")
        self.decls.append(
            f"static struct lind_layout {layout_name} = {{ .kind = LIND_LO_STRUCT, "
            f".nfields = {len(fields)}, .fields = {fields_name}, "
            f'.struct_size = {st.get("size", 0)} }};'
        )
        return layout_name

    def ret_spec_body(self, ret):
        kind = ret.get("kind", "scalar")
        c = RET_KIND.get(kind, "LIND_RET_SCALAR")
        parts = [f".kind = {c}"]
        if kind in ("ptr_alias_arg", "ptr_into_arg"):
            parts.append(f'.alias_arg_index = {ret.get("alias_arg", 0)}')
        if kind == "handle":
            parts.append(f'.handle_class = {self._handle_class_id(ret.get("handle_class","void"))}')
        return ", ".join(parts)

    def emit_function_spec(self, fn):
        """Emit the lind_marshal_spec for one function; return spec var name."""
        name = fn["name"]
        args = fn.get("args", [])
        ret = fn.get("ret", {"kind": "void"})
        ret_body = self.ret_spec_body(ret)
        arg_inits = []
        for a in args:
            arg_inits.append(f"        {{ {self.arg_spec_body(a)} }},")
        spec_name = f"spec_{name}"
        body = (
            f"static struct lind_marshal_spec {spec_name} = {{\n"
            f"    .nargs = {len(args)},\n"
            f"    .args = {{\n" + "\n".join(arg_inits) + "\n    },\n"
            f"    .ret = {{ {ret_body} }},\n"
            f"}};"
        )
        return spec_name, body


GRATE_TEMPLATE = r'''// AUTO-GENERATED by tools/marshal-gen/gen_grate.py — do not edit by hand.
// Auto-interposition grate for {lib_name}: registers a generic marshalling
// handler for every inference-marshalable function, each bound to (spec, &real_fn).
//
// Compile:
//   lind-clang -s --compile-grate --fpcast-emu {lib_name}_auto_grate.c -- -I<lind_marshal.h dir> <{lib_name}.a>
// Run (from lindfs/):
//   lind-wasm --preload env=/lib/{lib_name}.so grates/{lib_name}_auto_grate.cwasm <app...>
#include <lind_syscall.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "lind_marshal.h"

// --- real library functions (defined via static-linked {lib_name}.a) ---
// K&R-style extern decls: only need them callable/address-takeable; fpcast-emu
// adapts the ABI at the uniform-pointer call site.
{externs}

// --- per-function marshalling specs (translated from {lib_name}.marshal.json) ---
{specs}

// --- per-symbol dispatch contexts ---
struct libctx {{ const struct lind_marshal_spec *spec; void *real_fn; }};

struct reg_entry {{ const char *name; struct libctx ctx; }};
static struct reg_entry g_table[] = {{
{table}
}};
#define G_TABLE_N ((int)(sizeof(g_table)/sizeof(g_table[0])))

// --- generic dispatcher: registered value is a struct libctx* ---
int64_t pass_fptr_to_wt(uint64_t ctx_ptr_u64, uint64_t cageid,
                    uint64_t a1, uint64_t a1c, uint64_t a2, uint64_t a2c,
                    uint64_t a3, uint64_t a3c, uint64_t a4, uint64_t a4c,
                    uint64_t a5, uint64_t a5c, uint64_t a6, uint64_t a6c) {{
    if (ctx_ptr_u64 == 0) {{ fprintf(stderr, "[{lib_name}-grate] null ctx\n"); assert(0); }}
    struct libctx *ctx = (struct libctx *)(uintptr_t)ctx_ptr_u64;
    uint64_t raw[6]   = {{ a1, a2, a3, a4, a5, a6 }};
    uint64_t cages[6] = {{ a1c, a2c, a3c, a4c, a5c, a6c }};
    uint64_t src = 0;
    for (int i = 0; i < 6 && src == 0; i++) src = cages[i];
    // recover the enclosing reg_entry (ctx is always &g_table[i].ctx) for its name
    struct reg_entry *_re =
        (struct reg_entry *)((char *)ctx - offsetof(struct reg_entry, ctx));
    // Full 64-bit lind_marshal_dispatch() result, not truncated to int, so
    // 64-bit scalar returns (double, int64_t, pointers) survive the round
    // trip through the wasm-level call/host dispatch/wasm-level return --
    // matches lind_marshal.h's LIND_DEFINE_MARSHAL_HANDLER.
    return (int64_t)lind_marshal_dispatch(ctx->real_fn, ctx->spec, src, cageid,
                                      raw, ctx->spec->nargs, _re->name);
}}

int main(int argc, char *argv[]) {{
    if (argc < 2) {{ fprintf(stderr, "Usage: %s <app> [args...]\n", argv[0]); assert(0); }}
    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) {{ perror("fork"); assert(0); }}
    if (pid == 0) {{
        int cageid = getpid();
        int ok = 0, fail = 0;
        for (int i = 0; i < G_TABLE_N; i++) {{
            int r = register_lib_handler(cageid, "env", g_table[i].name,
                        grateid, (uint64_t)(uintptr_t)&g_table[i].ctx);
            if (r == 0) ok++;
            else {{ fail++; fprintf(stderr, "[{lib_name}-grate] register %s failed: %d\n",
                                    g_table[i].name, r); }}
        }}
        fprintf(stderr, "[{lib_name}-grate] registered %d/%d handlers\n", ok, ok + fail);
        if (execv(argv[1], &argv[1]) == -1) {{ perror("execv"); assert(0); }}
    }}
    int status;
    while (wait(&status) > 0) {{}}
    int ce = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    fprintf(stderr, "[{lib_name}-grate] app exited %d\n", ce);
    return ce == 0 ? 0 : 1;
}}
'''


FREESTANDING_TEMPLATE = r'''// AUTO-GENERATED by tools/marshal-gen/gen_grate.py (freestanding) — do not edit.
// Full-library auto-interposition grate. Includes NO libc headers so it can
// extern-declare every interposed libc symbol without prototype conflicts; the
// grate's own helpers are declared by hand below.
#define LIND_MARSHAL_NO_LIBC_HEADERS
#include "lind_marshal.h"   // -> lind_syscall.h (register_lib_handler, copy_data_between_cages), stdint, stddef

// grate's own helpers (K&R extern long; compatible with the table's extern decls).
// No stdio: the grate stays SILENT so it doesn't perturb a test's stdout/stderr,
// and returns the child's real exit code, so the harness sees the unmodified test.
extern long getpid();
extern long fork();
extern long wait();
extern long execv();
#define LIND_WIFEXITED(s)   (((s) & 0x7f) == 0)
#define LIND_WEXITSTATUS(s) (((s) >> 8) & 0xff)

// --- real library functions (defined via static-linked libc) ---
{externs}

// --- per-function marshalling specs ---
{specs}

struct libctx {{ const struct lind_marshal_spec *spec; void *real_fn; }};
struct reg_entry {{ const char *name; struct libctx ctx; }};
static struct reg_entry g_table[] = {{
{table}
}};
#define G_TABLE_N ((int)(sizeof(g_table)/sizeof(g_table[0])))

int64_t pass_fptr_to_wt(uint64_t ctx_ptr_u64, uint64_t cageid,
                    uint64_t a1, uint64_t a1c, uint64_t a2, uint64_t a2c,
                    uint64_t a3, uint64_t a3c, uint64_t a4, uint64_t a4c,
                    uint64_t a5, uint64_t a5c, uint64_t a6, uint64_t a6c) {{
    if (ctx_ptr_u64 == 0) __builtin_trap();
    struct libctx *ctx = (struct libctx *)(uintptr_t)ctx_ptr_u64;
    uint64_t raw[6]   = {{ a1, a2, a3, a4, a5, a6 }};
    uint64_t cages[6] = {{ a1c, a2c, a3c, a4c, a5c, a6c }};
    uint64_t src = 0;
    for (int i = 0; i < 6 && src == 0; i++) src = cages[i];
    // recover the enclosing reg_entry (ctx is always &g_table[i].ctx) for its name
    struct reg_entry *_re =
        (struct reg_entry *)((char *)ctx - offsetof(struct reg_entry, ctx));
    // Full 64-bit lind_marshal_dispatch() result, not truncated to int, so
    // 64-bit scalar returns (double, int64_t, pointers) survive the round
    // trip through the wasm-level call/host dispatch/wasm-level return --
    // matches lind_marshal.h's LIND_DEFINE_MARSHAL_HANDLER.
    return (int64_t)lind_marshal_dispatch(ctx->real_fn, ctx->spec, src, cageid,
                                      raw, ctx->spec->nargs, _re->name);
}}

int main(int argc, char *argv[]) {{
    if (argc < 2) __builtin_trap();
    long grateid = getpid();
    long pid = fork();
    if (pid < 0) __builtin_trap();
    if (pid == 0) {{
        long cageid = getpid();
        for (int i = 0; i < G_TABLE_N; i++) {{
            register_lib_handler((uint64_t)cageid, "env", g_table[i].name,
                        (uint64_t)grateid, (uint64_t)(uintptr_t)&g_table[i].ctx);
        }}
        execv(argv[1], &argv[1]);
        __builtin_trap();  // execv only returns on failure
    }}
    int status = 0;
    while (wait(&status) > 0) {{}}
    return LIND_WIFEXITED(status) ? LIND_WEXITSTATUS(status) : 1;
}}
'''


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("json", help="<lib>.marshal.json")
    ap.add_argument("--lib-name", required=True, help="e.g. libz")
    ap.add_argument("--out", required=True, help="output .c file")
    ap.add_argument("--only", default="", help="comma-separated subset of function names to interpose")
    ap.add_argument("--no-externs", action="store_true",
                    help="don't emit extern decls (real fns come from included headers, e.g. libc)")
    ap.add_argument("--include", default="", help="comma-separated extra headers to #include")
    ap.add_argument("--freestanding", action="store_true",
                    help="emit a header-free grate that extern-declares every symbol "
                         "(for interposing all of libc without prototype conflicts)")
    args = ap.parse_args()

    d = json.load(open(args.json))
    marshal_fns = [f for f in d["functions"] if f.get("decision") == "marshal"]
    # Drop functions whose spec uses unsupported features -> force_local (safe fallback).
    dropped = [f["name"] for f in marshal_fns if not is_marshalable(f)]
    fns = [f for f in marshal_fns if is_marshalable(f)]
    if args.only:
        want = {n.strip() for n in args.only.split(",") if n.strip()}
        fns = [f for f in fns if f["name"] in want]
        missing = want - {f["name"] for f in fns}
        if missing:
            print(f"[gen_grate] WARNING: not marshalable/absent: {sorted(missing)}", file=sys.stderr)

    em = Emitter()
    externs, specs, table = [], [], []
    emit_externs = args.freestanding or not args.no_externs
    for f in fns:
        name = f["name"]
        if emit_externs:
            externs.append(f"extern long {name}();")
        spec_name, spec_body = em.emit_function_spec(f)
        specs.append(spec_body)
        table.append(
            f'    {{ "{name}", {{ &{spec_name}, (void *)(uintptr_t)&{name} }} }},'
        )

    extern_block = "\n".join(externs) if externs else \
        "// (no extern decls: real functions come from the included headers)"
    if args.include:
        extern_block = "\n".join(f"#include <{h.strip()}>" for h in args.include.split(",")) \
            + "\n" + extern_block

    template = FREESTANDING_TEMPLATE if args.freestanding else GRATE_TEMPLATE
    out = template.format(
        lib_name=args.lib_name,
        externs=extern_block,
        specs="\n".join(em.decls + specs),
        table="\n".join(table),
    )
    with open(args.out, "w") as fh:
        fh.write(out)
    n_force = sum(1 for f in d["functions"] if f.get("decision") == "force_local")
    print(f"[gen_grate] {len(fns)} marshalable handlers, "
          f"{n_force} inference-force_local + {len(dropped)} dropped-unsupported (un-interposed)")
    if dropped:
        print(f"[gen_grate] dropped to force_local (unsupported features): "
              f"{len(dropped)} fns, e.g. {sorted(dropped)[:8]}")
    print(f"[gen_grate] wrote {args.out}")


if __name__ == "__main__":
    main()
