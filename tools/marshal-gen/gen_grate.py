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
    "stride_vector": "LIND_SIZE_STRIDE_VECTOR",
    "expr": "LIND_SIZE_EXPR",
}

# General lind_extent_expr tree node kinds (see lind_marshal.h's own doc on
# that type) -- the vocabulary import_openblas_inference.py's
# lower_operand_tree already lowers an LLM extent formula into, one-to-one.
# Every key here always carries an "op" field matching it.
EXTENT_EXPR_OP = {
    "constant": "LIND_EXPR_CONSTANT",
    "arg_value": "LIND_EXPR_ARG_VALUE",
    "arg_pointee_i32": "LIND_EXPR_ARG_POINTEE_I32",
    "abs": "LIND_EXPR_ABS",
    "product": "LIND_EXPR_PRODUCT",
    "max": "LIND_EXPR_MAX",
    "add": "LIND_EXPR_ADD",
    "ceil_divide": "LIND_EXPR_CEIL_DIVIDE",
}
EXTENT_EXPR_LEAF_TYPE = {
    "i32": "LIND_EXTENT_LEAF_I32",
    "u32": "LIND_EXTENT_LEAF_U32",
    "i64": "LIND_EXTENT_LEAF_I64",
    "u64": "LIND_EXTENT_LEAF_U64",
}
# A constant leaf's value must be representable the same way
# lind_marshal.h's own LIND_EXPR_CONSTANT case requires (it stores the value
# as uint64_t but evaluates the whole tree in int64_t, so a value above
# INT64_MAX would abort there even though it fits the field's own type).
EXTENT_EXPR_CONST_VALUE_MAX = (1 << 63) - 1

# Must match tests/grate-tests/lib-interpose/lind_marshal.h's
# LIND_EXTENT_EXPR_MAX_DEPTH/_MAX_NODES exactly -- duplicated, not shared,
# the same pattern (and the same reason) as LIND_RAW_ARGS_MAX below;
# tests/grate-tests/lib-interpose/check_extent_expr_bounds_consistency.sh
# fails the moment the two copies disagree.
LIND_EXTENT_EXPR_MAX_DEPTH = 16
LIND_EXTENT_EXPR_MAX_NODES = 64
# StrideVector extent operands: whether a raw wasm argument slot IS the
# value, points to it, or the operand is a compile-time constant with no
# argument behind it at all (see ParamTree.h's ExtentSource / lind_marshal.h's
# lind_extent_source). The by-value/by-reference distinction is required
# because classic Fortran BLAS passes every scalar by reference (`int *N`),
# unlike CBLAS's by-value `int n` -- the two cannot share one representation
# without losing this distinction. "constant" covers an ordinary contiguous
# walk (stride==1) or a fixed compile-time interleave, proven directly from
# the loop's own IR with no caller argument involved.
EXTENT_SOURCE = {
    "value": "LIND_EXTENT_VALUE",
    "pointee_i32": "LIND_EXTENT_POINTEE_I32",
    "constant": "LIND_EXTENT_CONSTANT",
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
                  "cstr", "ptr_array", "stride_vector", "expr"}

# V1's call-site transport is fixed-arity at this many raw wasm-level
# argument/cage-id pairs (pass_fptr_to_wt / register_lib_handler /
# lind_marshal_dispatch — see tests/grate-tests/lib-interpose/lind_marshal.h's
# LIND_RAW_ARGS_MAX). This is THIS tool's own, primary enforcement of that
# width, not a redundant backstop: marshal-infer (Infer.cpp's kMaxRawArgSlots/
# annotateWideRawArgSlots) only ANNOTATES a wider function's slot count now,
# it no longer force_locals purely for width — the variable-width V2
# transport can carry such a function via gen_v2_adapter.py, which reuses this same
# unmarshalable_reason() with max_args=None. A "marshal"-decision function
# wider than this IS expected input here, not stale/hand-edited JSON; this
# check is what keeps V1 generation specifically from picking it up (V1's
# own transport genuinely cannot carry it — generating a handler for one
# would compile fine and only abort, taking the whole grate process down
# with it, on the function's first real call).
#
# This width is duplicated, not shared, across marshal-infer, gen_grate.py,
# lind_marshal.h, and linker.rs — all four must be changed together;
# tests/grate-tests/lib-interpose/check_raw_arg_slot_consistency.sh fails the
# moment any of them disagree.
LIND_RAW_ARGS_MAX = 6


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


# _lind_eval_extent_operand casts a CONSTANT operand's value to int32_t
# (lind_marshal.h); a const_value above this would silently become negative
# on that cast, which the runtime's own negative-stride check would then
# (correctly, but confusingly -- the config never asked for a negative
# stride) reject at dispatch time instead of here at generation time.
EXTENT_CONST_VALUE_MAX = 0x7FFFFFFF


def _valid_extent_operand(o, nargs):
    """True iff `o` is a well-formed StrideVector size_operand/stride_operand:
    either a dict with an in-range integer arg_index and source "value"/
    "pointee_i32" and no const_value, or one with source "constant" and a
    positive, in-range const_value and no MEANINGFUL arg_index -- the two
    shapes are mutually exclusive, not a fallback chain. marshal-infer's own
    JSON always includes an "arg_index" key (its usual -1 "not applicable"
    sentinel for a constant operand -- see ParamTree.h's ExtentOperand), so
    -1 is tolerated there; any OTHER value alongside source=="constant" is a
    self-contradictory spec (which one does the runtime honor?), not a
    harmless redundancy, and rejected the same as a const_value present on
    a non-constant operand. Malformed metadata here (missing, wrong-typed,
    out-of-range, an unrecognized source string, or a field present on the
    shape it doesn't belong to) must never be silently repaired into a
    valid-looking but wrong handler -- see EXTENT_SOURCE's own comment on
    why source can't be guessed."""
    if not isinstance(o, dict):
        return False
    src = o.get("source")
    if src not in EXTENT_SOURCE:
        return False
    if src == "constant":
        if o.get("arg_index", -1) != -1:
            return False
        v = o.get("const_value")
        return (isinstance(v, int) and not isinstance(v, bool)
                and 0 < v <= EXTENT_CONST_VALUE_MAX)
    if "const_value" in o:
        return False
    idx = o.get("arg_index")
    return isinstance(idx, int) and not isinstance(idx, bool) and 0 <= idx < nargs


_EXTENT_EXPR_LEAF_TYPE_WIDTH = {"i32": 4, "u32": 4, "i64": 8, "u64": 8}


def _scalar_arg_matches_leaf_type(arg, leaf_type):
    """True iff `arg` (a gen_grate args[] scalar entry) is exactly the
    width an arg_value leaf's `leaf_type` requires AND, for an unsigned
    leaf_type, the schema positively asserts the argument is unsigned.
    float/double scalars never match ANY leaf_type, regardless of width:
    lind_extent_expr's leaf types (i32/u32/i64/u64) are all plain-integer
    reinterpretations of the raw argument slot, and a float/double's bit
    pattern read that way is not a sensible count in any width -- it's an
    unrelated reinterpretation of totally different bits, not merely the
    wrong size. An integer scalar of the WRONG width (e.g. an 8-byte
    `long` argument declared as a 4-byte `i32` leaf) would silently read
    only the low (or, depending on real endianness/ABI, an unrelated)
    half of the real value -- also rejected, not merely "probably fine".

    gen_grate's own scalar schema has no explicit signed/unsigned type
    string distinct from plain `"int"` -- import_openblas_inference.py's
    own lower_llm_function always maps a scalar llvm_type to `"int"`
    (i32), `"float"`, or `"double"`, with no `"uint"`/`"unsigned"`
    counterpart anywhere in the schema today. So a plain `"int"` scalar
    only ever matches a SIGNED leaf_type (`i32`/`i64`): `u32`/`u64` are
    rejected even when the width matches, since nothing in the schema
    positively asserts the argument is actually unsigned -- widen this if
    the schema ever gains an explicit unsigned scalar type."""
    if arg.get("type") != "int":
        return False
    if arg.get("size") != _EXTENT_EXPR_LEAF_TYPE_WIDTH.get(leaf_type):
        return False
    return leaf_type in ("i32", "i64")


def _ptr_arg_has_proven_int32_pointee(arg):
    """True iff `arg` (a gen_grate args[] ptr entry) carries an explicit,
    importer-PROVEN annotation (`int32_pointee_proven`) that its pointee
    is exactly a 4-byte int -- the only pointer shape arg_pointee_i32 may
    safely dereference. Deliberately does NOT infer this from
    `size_kind`/`const_size` alone: a fixed 4-byte buffer is not
    necessarily an int (a real `float *` argument is equally 4 bytes per
    element), and `pointee[0]["type"]` is documented as informational-only
    metadata for some extent kinds (see import_openblas_inference.py's own
    lower_pointer_argument doc), never a safety-critical proof on its own.
    Only the explicit annotation -- set by the importer's own name-profile
    verification (`_INT_SCALAR_NAMES`), not re-derived here -- is trusted."""
    return arg.get("kind") == "ptr" and arg.get("int32_pointee_proven") is True


def _valid_extent_expr_tree(node, args, depth=0, nodes_remaining=None):
    """None iff `node` is a well-formed lind_extent_expr tree this generator
    can safely emit: every op recognized, every leaf's arg_index in range
    AND the referenced argument's own ABI kind/width consistent with that
    leaf (arg_value must name a scalar of EXACTLY the width/float-ness its
    leaf_type requires, with signedness checked where the schema can
    actually represent it -- reading a pointer's raw bits, a mismatched-
    width integer, or a float/double's bits as a count would all silently
    misinterpret the argument; arg_pointee_i32 must name a ptr carrying an
    explicit, importer-proven `int32_pointee_proven` annotation -- there is
    nothing to dereference on a scalar, and neither a fixed byte count nor
    a descriptive type label alone proves the pointee is actually an int,
    e.g. a real `float *` is equally 4 bytes per element), a recognized
    leaf_type for arg_value, every constant leaf's value non-negative and
    representable, and the WHOLE tree within the runtime's
    own depth/node-count ceiling (LIND_EXTENT_EXPR_MAX_DEPTH/_MAX_NODES
    above) -- checked here, at generation time, in ADDITION to (not instead
    of) the runtime's own identical checks in _lind_eval_extent_expr_depth:
    this gives a precise, actionable reason tied to the actual JSON at the
    point generation fails, while the runtime check remains the real
    enforcement boundary against any other caller of arg_spec_body/
    emit_extent_expr that skips this validator. The ABI-kind check in
    particular matters even though import_openblas_inference.py already
    enforces the matching rule on its own lowered output (lower_operand_tree's
    own llvm_type checks): the generator is an INDEPENDENT trust boundary,
    not merely a second copy of the importer's check, since nothing stops
    a different or hand-edited marshal.json from reaching this code without
    ever passing through that importer at all. Otherwise a short, specific
    reason -- never silently truncates, defaults, or repairs a malformed
    tree.

    `args`: the function's own per-argument spec list (the same
    {"kind": ...} dicts f.get("args", []) holds), used to both range-check
    a leaf's arg_index and confirm the referenced argument's ABI kind, or
    None to skip BOTH checks entirely -- the same "primary gate has the
    real argument list, callers without one only get a weaker backstop"
    split unmarshalable_reason/arg_spec_body already have for
    _valid_extent_operand above (unmarshalable_reason is always called with
    the real args list; arg_spec_body, which doesn't receive one, passes
    None)."""
    if nodes_remaining is None:
        nodes_remaining = [LIND_EXTENT_EXPR_MAX_NODES]
    if not isinstance(node, dict):
        return f"expected an operand object, got {node!r}"
    if depth >= LIND_EXTENT_EXPR_MAX_DEPTH:
        return "exceeds maximum depth"
    if nodes_remaining[0] <= 0:
        return "exceeds maximum node count"
    nodes_remaining[0] -= 1

    op = node.get("op")
    if op not in EXTENT_EXPR_OP:
        return f"unsupported operator {op!r}"

    if op == "constant":
        v = node.get("value")
        if (not isinstance(v, int) or isinstance(v, bool)
                or not (0 <= v <= EXTENT_EXPR_CONST_VALUE_MAX)):
            return f"constant leaf has invalid value {v!r}"
        return None

    if op in ("arg_value", "arg_pointee_i32"):
        idx = node.get("arg_index")
        if not isinstance(idx, int) or isinstance(idx, bool) or idx < 0:
            return f"argument index {idx!r} out of range"
        if args is not None:
            if idx >= len(args):
                return f"argument index {idx!r} out of range (nargs={len(args)})"
            arg = args[idx]
            arg_kind = arg.get("kind", "scalar")
            if op == "arg_value":
                if arg_kind != "scalar":
                    return (f"arg_value references argument {idx}, whose kind is "
                            f"{arg_kind!r}, not scalar -- refusing to read a "
                            f"non-scalar argument's raw bits as a count")
                leaf_type = node.get("leaf_type")
                if (leaf_type in EXTENT_EXPR_LEAF_TYPE
                        and not _scalar_arg_matches_leaf_type(arg, leaf_type)):
                    return (f"arg_value references argument {idx} (type="
                            f"{arg.get('type')!r}, size={arg.get('size')!r}), which "
                            f"does not match leaf_type {leaf_type!r} -- refusing to "
                            f"read a mismatched-width or non-integer scalar's raw "
                            f"bits as this leaf's value")
            if op == "arg_pointee_i32" and arg_kind != "ptr":
                return (f"arg_pointee_i32 references argument {idx}, whose kind is "
                        f"{arg_kind!r}, not ptr -- refusing to dereference a "
                        f"non-pointer argument")
            if op == "arg_pointee_i32" and arg_kind == "ptr" and not _ptr_arg_has_proven_int32_pointee(arg):
                return (f"arg_pointee_i32 references argument {idx}, which has no "
                        f"importer-proven int32_pointee_proven annotation -- refusing "
                        f"to dereference a pointer whose pointee can't be confirmed "
                        f"to be a 4-byte int")
        if op == "arg_value" and node.get("leaf_type") not in EXTENT_EXPR_LEAF_TYPE:
            return f"arg_value leaf has unsupported leaf_type {node.get('leaf_type')!r}"
        return None

    if op == "abs":
        operand = node.get("operand")
        if operand is None:
            return "abs missing its operand"
        return _valid_extent_expr_tree(operand, args, depth + 1, nodes_remaining)

    # Binary: product/max/add/ceil_divide.
    lhs, rhs = node.get("lhs"), node.get("rhs")
    if lhs is None or rhs is None:
        return f"{op} missing lhs/rhs"
    r = _valid_extent_expr_tree(lhs, args, depth + 1, nodes_remaining)
    if r:
        return r
    return _valid_extent_expr_tree(rhs, args, depth + 1, nodes_remaining)


def unmarshalable_reason(f, warn=False, max_args=LIND_RAW_ARGS_MAX):
    """None iff the runtime can faithfully marshal every part of this spec;
    otherwise a short, specific, actionable reason why not -- naming the
    exact argument/pointee and the exact malformed field, never just "no".

    `max_args`: the raw-ABI-slot cap to enforce, or None to skip that check
    entirely. gen_grate.py's own V1 dispatch is fixed at LIND_RAW_ARGS_MAX
    (the default here); gen_v2_adapter.py reuses this same per-argument walk
    for its variable-width adapters, which have no such fixed cap."""
    name = f.get("name", "")
    if name in NEVER_INTERPOSE:
        return "function is in NEVER_INTERPOSE (control-flow terminator / exec-family / static-link-blocked / binaryen bug)"
    if name in FD_FUNCS:
        return "function operates on a per-cage file descriptor (see FD_FUNCS)"
    for s in NEVER_INTERPOSE_SUBSTR:
        if s in name:
            return f"function name contains NEVER_INTERPOSE_SUBSTR {s!r}"
    # `args` is already one JSON entry per raw wasm-level ABI slot (sret and
    # multi-slot params are pre-flattened by marshal-infer), so its length IS
    # the raw slot count -- see LIND_RAW_ARGS_MAX. Inference marks a wide
    # function "marshal" too now (it only ANNOTATES the width, in a warning --
    # see Infer.cpp's annotateWideRawArgSlots): with the DEFAULT max_args
    # (V1's own fixed 6-slot dispatch), reaching this branch is the NORMAL,
    # expected outcome for a function V2 could carry but V1 cannot, not a
    # JSON bug -- gen_v2_adapter.py picks these up instead (max_args=None
    # skips this check entirely there). Generating a V1 handler for one here
    # would compile fine and only abort -- taking the whole grate process
    # down with it -- on the function's first real call, so this exclusion is
    # still enforced, just no longer described as an anomaly.
    top_args = f.get("args", [])
    nargs = len(top_args)
    if max_args is not None and nargs > max_args:
        reason = (f"needs {nargs} raw ABI slots, exceeding the interposition "
                  f"transport's {max_args}-slot capacity (V1-only; a "
                  f"variable-width V2 adapter can still be generated via "
                  f"gen_v2_adapter.py)")
        if warn:
            print(f"[gen_grate] REJECTING {name}: {reason}", file=sys.stderr)
        return reason
    ret = f.get("ret") or {}
    r = ret.get("kind")
    if r is not None and r not in SUPPORTED_RET:
        return f"unsupported return kind {r!r} (ptr_alloc/ptr_to_static/ptr_into_cursor hand the app a grate-cage pointer it can't dereference)"
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

    def walk(n, path, top_level):
        if n.get("cursor"):                       # strsep-style cursor: not implemented
            return f"{path}: cursor-style pointee not implemented"
        if n.get("kind") == "ptr":
            sk = n.get("size_kind")
            if sk not in SUPPORTED_SIZE:
                return f"{path}: unsupported size_kind {sk!r} -- can't size the copy safely"
            if sk == "stride_vector":
                const_size = n.get("const_size")
                if (not isinstance(const_size, int) or isinstance(const_size, bool)
                        or const_size <= 0):
                    return f"{path}: stride_vector has invalid const_size {const_size!r}"
                # Two mutually exclusive operand shapes: the legacy
                # single-leaf lind_extent_operand pair (size_operand/
                # stride_operand), or a general lind_extent_expr tree pair
                # (size_operand_expr/stride_operand_expr -- import_
                # openblas_inference.py's own output, always both together,
                # e.g. for a stride that must be abs()'d). Exactly one of
                # the two pairs must be present; one tree key present
                # without its partner is self-contradictory, not a
                # harmless partial spec.
                has_size_expr = "size_operand_expr" in n
                has_stride_expr = "stride_operand_expr" in n
                if has_size_expr != has_stride_expr:
                    return (f"{path}: stride_vector has exactly one of "
                            f"size_operand_expr/stride_operand_expr -- both or "
                            f"neither, never one alone")
                if has_size_expr:
                    # The runtime's nested-struct-field switch (lind_marshal.h)
                    # only ever computes a sibling field's size via CSTR/
                    # FROM_ARG/CONST -- there is no raw_args array (and no
                    # general lind_extent_expr evaluation) in that context at
                    # all, so a tree-shaped stride_vector operand generated
                    # inside a struct field would compile fine and only abort
                    # on the function's first real call. Reject at generation
                    # time instead of expanding the (deliberately unexercised)
                    # nested runtime path -- see Gate 3's own notes on why
                    # nested expression support stays out of scope.
                    if not top_level:
                        return (f"{path}: stride_vector size_operand_expr/"
                                f"stride_operand_expr is not supported on a "
                                f"nested pointer field (only CSTR/FROM_ARG/CONST "
                                f"are)")
                    for label, key in (("size_operand_expr", "size_operand_expr"),
                                       ("stride_operand_expr", "stride_operand_expr")):
                        r = _valid_extent_expr_tree(n.get(key), top_args)
                        if r:
                            return f"{path}: stride_vector {label} {r}"
                else:
                    for label, key in (("size_operand", "size_operand"),
                                       ("stride_operand", "stride_operand")):
                        o = n.get(key)
                        if not _valid_extent_operand(o, nargs):
                            return f"{path}: stride_vector {label} malformed/out-of-range: {o!r}"
            if sk == "expr":
                # Same reasoning as stride_vector's tree-operand case just
                # above: the nested-struct-field runtime switch has no
                # LIND_SIZE_EXPR case at all, only CSTR/FROM_ARG/CONST.
                if not top_level:
                    return (f"{path}: size_kind 'expr' is not supported on a "
                            f"nested pointer field (only CSTR/FROM_ARG/CONST are)")
                r = _valid_extent_expr_tree(n.get("size_expr"), top_args)
                if r:
                    return f"{path}: size_expr {r}"
        for i, ch in enumerate(n.get("pointee") or []):
            r = walk(ch, f"{path}.pointee[{i}]", False)
            if r:
                return r
        for i, ch in enumerate(n.get("fields") or []):
            r = walk(ch, f"{path}.fields[{i}]", False)
            if r:
                return r
        return None

    for i, a in enumerate(top_args):
        r = walk(a, f"arg{i}", True)
        if r:
            return r
    return None


def is_marshalable(f):
    """True iff the runtime can faithfully marshal every part of this spec."""
    return unmarshalable_reason(f, warn=True) is None

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
        if sk == "expr":
            # unmarshalable_reason() is the primary gate (it also
            # range-checks every leaf's arg_index against the function's
            # real arg count, which isn't known here) -- this call is a
            # defense-in-depth backstop for any direct caller of
            # arg_spec_body that skips it, the same division of
            # responsibility stride_vector's own operand validation below
            # already has.
            node = a.get("size_expr")
            reason = _valid_extent_expr_tree(node, None)
            if reason:
                raise ValueError(f"size_expr missing/malformed: {reason}")
            expr_name = self.emit_extent_expr(node)
            parts.append(f".size_expr = &{expr_name}")
        if sk == "stride_vector":
            # is_marshalable() is the primary gate (it also range-checks
            # arg_index against the function's real arg count, which isn't
            # known here) -- this is a defense-in-depth backstop for any
            # direct caller of arg_spec_body/emit_function_spec that skips
            # it. Malformed metadata (missing, wrong-typed, or an
            # unrecognized source) must fail loudly, never silently default
            # to arg 0 / LIND_EXTENT_VALUE: that would emit a valid-looking
            # but wrong handler instead of refusing to generate one.
            def operand(o, label):
                if not isinstance(o, dict):
                    raise ValueError(f"stride_vector {label} missing/malformed: {o!r}")
                src = o.get("source")
                if src not in EXTENT_SOURCE:
                    raise ValueError(f"stride_vector {label} has unknown source: {src!r}")
                if src == "constant":
                    if o.get("arg_index", -1) != -1:
                        raise ValueError(f"stride_vector {label} has a real arg_index "
                                          f"alongside source=constant -- self-contradictory: {o!r}")
                    v = o.get("const_value")
                    if (not isinstance(v, int) or isinstance(v, bool)
                            or not (0 < v <= EXTENT_CONST_VALUE_MAX)):
                        raise ValueError(f"stride_vector {label} has invalid const_value: {v!r}")
                    return f'{{ .source = {EXTENT_SOURCE[src]}, .const_value = {v} }}'
                if "const_value" in o:
                    raise ValueError(f"stride_vector {label} has const_value but "
                                      f"source={src!r}, not constant -- self-contradictory: {o!r}")
                idx = o.get("arg_index")
                if not isinstance(idx, int) or isinstance(idx, bool) or idx < 0:
                    raise ValueError(f"stride_vector {label} has invalid arg_index: {idx!r}")
                return f'{{ .arg_index = {idx}, .source = {EXTENT_SOURCE[src]} }}'
            const_size = a.get("const_size")
            if not isinstance(const_size, int) or isinstance(const_size, bool) or const_size <= 0:
                raise ValueError(f"stride_vector has invalid const_size: {const_size!r}")
            # Two mutually exclusive operand shapes -- see
            # unmarshalable_reason's own walk() doc on the same split.
            has_size_expr = "size_operand_expr" in a
            has_stride_expr = "stride_operand_expr" in a
            if has_size_expr != has_stride_expr:
                raise ValueError(
                    "stride_vector has exactly one of size_operand_expr/"
                    "stride_operand_expr -- both or neither, never one alone"
                )
            if has_size_expr:
                size_node, stride_node = a["size_operand_expr"], a["stride_operand_expr"]
                for node, label in ((size_node, "size_operand_expr"), (stride_node, "stride_operand_expr")):
                    reason = _valid_extent_expr_tree(node, None)
                    if reason:
                        raise ValueError(f"stride_vector {label} missing/malformed: {reason}")
                size_name = self.emit_extent_expr(size_node)
                stride_name = self.emit_extent_expr(stride_node)
                parts.append(f".size_operand_expr = &{size_name}")
                parts.append(f".stride_operand_expr = &{stride_name}")
            else:
                parts.append(f'.size_operand = {operand(a.get("size_operand"), "size_operand")}')
                parts.append(f'.stride_operand = {operand(a.get("stride_operand"), "stride_operand")}')
            parts.append(f'.const_size = {const_size}')

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

    def emit_extent_expr(self, node):
        """Emit one lind_extent_expr tree node (and, recursively, every
        child it has) into self.decls, children before their parent -- a
        parent's own initializer takes a child's address, which C requires
        to already be a visible prior declaration at file scope. Returns
        the new node's own C variable name (not `&name`; callers embed it
        the same way emit_layout/emit_element's own callers do). Assumes
        `node` already passed _valid_extent_expr_tree -- the same division
        of responsibility arg_spec_body's other nested emitters have."""
        op = node["op"]
        c_op = EXTENT_EXPR_OP[op]
        name = self.uid("extentexpr")
        if op == "constant":
            init = f".kind = {c_op}, .const_value = {node['value']}ULL"
        elif op == "arg_value":
            init = (f'.kind = {c_op}, .arg_index = {node["arg_index"]}, '
                    f'.leaf_type = {EXTENT_EXPR_LEAF_TYPE[node["leaf_type"]]}')
        elif op == "arg_pointee_i32":
            init = f'.kind = {c_op}, .arg_index = {node["arg_index"]}'
        elif op == "abs":
            child_name = self.emit_extent_expr(node["operand"])
            init = f".kind = {c_op}, .lhs = &{child_name}"
        else:  # product / max / add / ceil_divide
            lhs_name = self.emit_extent_expr(node["lhs"])
            rhs_name = self.emit_extent_expr(node["rhs"])
            init = f".kind = {c_op}, .lhs = &{lhs_name}, .rhs = &{rhs_name}"
        self.decls.append(f"static const struct lind_extent_expr {name} = {{ {init} }};")
        return name

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
    if (ctx_ptr_u64 == 0) {{ fprintf(stderr, "[{lib_name}-grate] null ctx\n"); __builtin_trap(); }}
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
    if (argc < 2) {{ fprintf(stderr, "Usage: %s <app> [args...]\n", argv[0]); __builtin_trap(); }}
    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) {{ perror("fork"); __builtin_trap(); }}
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
        // Fail closed: an incomplete handler table means some interposed calls
        // would silently fall through to the app's own (uninterposed) library
        // instead of being marshalled -- a security-relevant gap, not merely a
        // degraded-diagnostics one. Abort startup rather than exec the app.
        if (fail > 0) {{
            fprintf(stderr, "[{lib_name}-grate] FATAL: %d/%d handler registrations failed — aborting startup\n",
                    fail, ok + fail);
            // __builtin_trap(), not assert(0): this is a security boundary (an
            // incomplete handler table means some calls would silently bypass
            // marshalling), not a debug-only invariant check -- it must not
            // disappear under -DNDEBUG. Matches the freestanding template.
            __builtin_trap();
        }}
        if (execv(argv[1], &argv[1]) == -1) {{ perror("execv"); __builtin_trap(); }}
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
            int r = register_lib_handler((uint64_t)cageid, "env", g_table[i].name,
                        (uint64_t)grateid, (uint64_t)(uintptr_t)&g_table[i].ctx);
            // Fail closed: an incomplete handler table means some interposed
            // calls would silently fall through to the app's own (uninterposed)
            // library instead of being marshalled -- abort startup rather than
            // exec the app in that state. (Silent, matching this template's own
            // no-libc/no-stdio convention -- see the file header comment.)
            if (r != 0) __builtin_trap();
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
    # A "marshal" decision is necessary but not sufficient for a generated
    # handler to exist: unmarshalable_reason() is the actual generator-side
    # gate, and its reason string is what makes a dropped function's cause
    # (malformed contract operand, unsupported return shape, ...) actionable
    # instead of a bare name in a list.
    dropped = [(f["name"], unmarshalable_reason(f)) for f in marshal_fns if not is_marshalable(f)]
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
    for dname, reason in sorted(dropped):
        print(f"[gen_grate] dropped to force_local: {dname}: {reason}", file=sys.stderr)
    print(f"[gen_grate] wrote {args.out}")


if __name__ == "__main__":
    main()
