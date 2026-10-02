#!/usr/bin/env python3
"""Generate real, lind_compile-buildable V2 (variable-width) grate adapters
from the SAME per-function marshal spec vocabulary gen_grate.py consumes
(issue #22).

Unlike gen_grate.py's ONE generic handler bound to a fixed six raw ABI slots
(pass_fptr_to_wt + lind_handler6_t + a K&R blind-cast real-function pointer,
ABI-adapted at the binary level by --fpcast-emu), each function here gets
its OWN generated C function with the function's EXACT lowered wasm
signature -- any number of i32/i64/f32/f64 params, not capped at six -- that
calls the real library function through a REAL, correctly-typed C prototype
and marshals pointers/handles/aliases through the SAME shared primitives
lind_marshal.h's V1 dispatch path uses:
  _lind_marshal_prepare_arg / _lind_marshal_finish_shadow /
  _lind_marshal_translate_return
No fpcast-emu trick is needed: every call this generator emits is an
ordinary, correctly-prototyped C function call, so a float/double value
needs (and gets) an explicit bit reinterpretation at the marshal boundary
(_lind_v2_bits_from_f64/f32, _lind_v2_f64_from_bits/f32_from_bits in
lind_marshal.h) instead of relying on binary-level ABI compatibility.

Reuses gen_grate.py's Emitter for arg_spec_body/ret_spec_body/emit_layout/
emit_element (the exact same lind_arg_spec/lind_layout/lind_return_spec C
literal syntax, already covering pointer IN/OUT/INOUT, nested structs,
handles, ptr_array, and alias returns) rather than re-deriving that mapping.

Usage:
  gen_v2_adapter.py <lib>.marshal.json --lib-name libz --out libz_v2_grate.c \\
      --only cblas_daxpy,daxpy_ [--manifest-version 2]

  # Self-contained grate (adapters + registration main()), the V2 replacement
  # for gen_grate.py's GRATE_TEMPLATE -- no hand-written driver needed:
  gen_v2_adapter.py <lib>.marshal.json --lib-name libz --out libz_v2_grate.c \\
      --emit-grate
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_grate import Emitter, unmarshalable_reason  # noqa: E402


def v2_unmarshalable_reason(f):
    """Like gen_grate.py's unmarshalable_reason, but with no raw-ABI-slot
    cap: a V2 adapter has its own exact lowered signature, not V1's fixed
    six-slot transport. Also rejects any scalar argument/return whose
    type/size this module can't positively confirm -- see
    _v2_scalar_shape_confirmed's own doc for why guessing one (as V1's
    ABI-agnostic dispatch can safely afford to) is unsound here: a
    static-inference "proven" record's own `ret` can be as bare as
    `{"kind": "scalar"}` (no type/size at all, since V1 never needed
    either -- fpcast-emu adapts the real ABI at the BINARY level
    regardless of the declared prototype), and defaulting that to int32
    would silently emit a WRONG, value-corrupting prototype for an
    actual float/double-returning function (confirmed against the real
    openblas-inference artifact: 41 of 151 V2-eligible functions --
    cblas_damax, cblas_dasum, damax_, ... -- have exactly this gap, every
    one already V1-generatable, so rejecting them from V2 loses no real
    coverage)."""
    reason = unmarshalable_reason(f, max_args=None)
    if reason:
        return reason
    for i, a in enumerate(f.get("args", [])):
        if a.get("kind", "scalar") == "scalar" and not _v2_scalar_shape_confirmed(a):
            return (f"arg{i}: scalar argument has no confirmed type/size "
                    f"(type={a.get('type')!r}, size={a.get('size')!r}) -- V2 must emit "
                    f"an exact, real C prototype, so an unconfirmed width/type can't "
                    f"be safely assumed")
    ret = f.get("ret") or {}
    ret_kind = ret.get("kind", "void")
    if (ret_kind not in ("void", "handle", "ptr_alias_arg", "ptr_into_arg")
            and not _v2_scalar_shape_confirmed(ret)):
        return (f"return value has no confirmed type/size (kind={ret_kind!r}, "
                f"type={ret.get('type')!r}, size={ret.get('size')!r}) -- same reasoning "
                f"as the scalar-argument check above")
    return None


def is_v2_marshalable(f):
    return v2_unmarshalable_reason(f) is None

MANIFEST_VERSION_EXPORT = "__lind_v2_manifest_version"
DEFAULT_MANIFEST_VERSION = 2

# register_lib_handler_v2's signature descriptor ("<manifest_version>:<params>:
# <results>") encodes each raw wasm-level value's KIND, not its C spelling --
# see lib_handler_table_v2.rs's parse_v2_type_char. wasm_scalar_ctype only
# ever returns these four spellings (ptr/handle already collapsed to
# uint32_t), so the mapping is total over its range.
V2_SIG_CHAR = {"uint32_t": "i", "int32_t": "i", "int64_t": "l", "float": "f", "double": "d"}


# Scalar (int/float/double) shapes this module can positively confirm are
# safe to emit an EXACT, real C prototype/call for. V1's own generic
# extern decl (gen_grate.py's `extern long name();`) never needs this,
# since fpcast-emu adapts the real ABI at the BINARY level regardless of
# the declared prototype -- so a static-inference "proven" record's
# `type`/`size` fields were never load-bearing there, and some real
# records (every "proven" scalar-returning function investigated here)
# omit them entirely. V2 has no such adaptation: it calls through a real,
# exactly-typed C prototype, so a missing or unrecognized type/size must
# be rejected, never defaulted.
_V2_CONFIRMED_SCALAR_SHAPES = {("int", 4), ("int", 8), ("float", 4), ("double", 8)}


def _v2_scalar_shape_confirmed(node):
    """True iff a scalar/ptr/handle `node` carries a type/size this module
    can positively map to a real wasm-level C type -- see
    _V2_CONFIRMED_SCALAR_SHAPES's own doc. ptr/handle are always
    confirmed (always a 32-bit address regardless of pointee)."""
    if node.get("kind", "scalar") in ("ptr", "handle"):
        return True
    return (node.get("type"), node.get("size")) in _V2_CONFIRMED_SCALAR_SHAPES


def wasm_scalar_ctype(node):
    """Maps a JSON scalar/ptr/handle node to its wasm-level C parameter
    type: the value KIND that actually matters for calling-convention
    correctness (i32/i64/f32/f64 are different wasm value kinds, not just
    different C type spellings -- getting this wrong corrupts the value,
    not just its label). A pointer or handle argument is always a 32-bit
    address at the wasm32 ABI level regardless of what it points to."""
    kind = node.get("kind", "scalar")
    if kind in ("ptr", "handle"):
        return "uint32_t"
    size, t = node.get("size"), node.get("type")
    if (t, size) not in _V2_CONFIRMED_SCALAR_SHAPES:
        # v2_unmarshalable_reason() is the primary gate; this is a
        # defense-in-depth backstop for any direct caller of
        # emit_v2_adapter that skips it -- never silently default to
        # int32_t, which would be right only by coincidence for a
        # genuinely-int node and WRONG (corrupting the real value) for an
        # unconfirmed float/double one.
        raise ValueError(f"scalar node has unconfirmed type/size: type={t!r}, size={size!r}")
    if size == 8 and t == "double":
        return "double"
    if size == 4 and t == "float":
        return "float"
    if size == 8:
        return "int64_t"
    return "int32_t"


def bits_expr(ctype, value_expr):
    """C expression converting a value of `ctype` to the uint64_t bit
    pattern the shared marshalling primitives use. Floating types need an
    explicit reinterpretation (see module doc); everything else is already
    an integer/address that widens safely."""
    if ctype == "double":
        return f"_lind_v2_bits_from_f64({value_expr})"
    if ctype == "float":
        return f"_lind_v2_bits_from_f32({value_expr})"
    return f"(uint64_t)({value_expr})"


def value_from_bits_expr(ctype, bits_expr_str):
    """Inverse of bits_expr: converts prepared uint64_t bits back to a real
    `ctype` value to pass into the real function call."""
    if ctype == "double":
        return f"_lind_v2_f64_from_bits({bits_expr_str})"
    if ctype == "float":
        return f"_lind_v2_f32_from_bits({bits_expr_str})"
    if ctype.endswith("*"):
        # uint64_t -> pointer needs the intermediate uintptr_t cast: a
        # direct (void *)(uint64_t) cast narrows on this 32-bit target and
        # is flagged (rightly) as a size-changing pointer cast otherwise.
        return f"({ctype})(uintptr_t)({bits_expr_str})"
    return f"({ctype})({bits_expr_str})"


class V2AdapterEmitter(Emitter):
    """Extends gen_grate.py's Emitter with per-symbol unrolled adapter
    generation instead of one generic runtime-dispatched handler."""

    def emit_v2_adapter(self, fn, manifest_version):
        """Returns (c_source_fragment, export_name, wasm_param_ctypes,
        wasm_result_ctype_or_None) for one function's generated adapter."""
        name = fn["name"]
        args = fn.get("args", [])
        ret = fn.get("ret", {"kind": "void"})
        n = len(args)
        export_name = f"__lind_v2_adapter_{name}"

        arg_ctypes = [wasm_scalar_ctype(a) for a in args]

        argspecs_name = f"v2_argspecs_{name}"
        if n > 0:
            arg_inits = "\n".join(f"    {{ {self.arg_spec_body(a)} }}," for a in args)
            self.decls.append(
                f"static struct lind_arg_spec {argspecs_name}[] = {{\n{arg_inits}\n}};"
            )

        ret_kind = ret.get("kind", "void")
        is_void = ret_kind == "void"
        retspec_name = f"v2_retspec_{name}"
        self.decls.append(
            f"static struct lind_return_spec {retspec_name} = {{ {self.ret_spec_body(ret)} }};"
        )

        # RET_HANDLE/RET_PTR_ALIAS_ARG/RET_PTR_INTO_ARG all resolve to a
        # grate/source-cage address (32-bit at the wasm level); only a
        # genuine LIND_RET_SCALAR return can be a float/double.
        if ret_kind in ("handle", "ptr_alias_arg", "ptr_into_arg"):
            ret_wasm_ctype = "uint32_t"
        elif is_void:
            ret_wasm_ctype = None
        else:
            ret_wasm_ctype = wasm_scalar_ctype(ret)

        # Real extern prototype: exact for scalar float/double (the only
        # types where getting this wrong corrupts the value); a plain
        # `void *` for every pointer/handle slot, since void* and T* share
        # an identical representation and calling-convention slot on this
        # target -- see the module doc for why this is safe without needing
        # each argument's exact pointee C type.
        real_extern_ctypes = ["void *" if c == "uint32_t" else c for c in arg_ctypes]
        if ret_kind in ("handle", "ptr_alias_arg", "ptr_into_arg"):
            real_ret_ctype = "void *"
        elif is_void:
            real_ret_ctype = "void"
        else:
            real_ret_ctype = wasm_scalar_ctype(ret)
        extern_decl = (
            f"extern {real_ret_ctype} {name}"
            f"({', '.join(real_extern_ctypes) if real_extern_ctypes else 'void'});"
        )

        params = ["uint64_t source_cage", "uint64_t grate_cage"]
        for i, ct in enumerate(arg_ctypes):
            params.append(f"{ct} raw{i}")
        params_decl = ", ".join(params)

        lines = []
        adapter_ret_ctype = ret_wasm_ctype if ret_wasm_ctype else "void"
        # Not `static`: a grate's exported functions are ordinary top-level
        # (non-static) symbols under this toolchain's dylink convention --
        # see gen_grate.py's pass_fptr_to_wt, exported the same way with no
        # special attribute. The explicit export_name attribute in main()'s
        # template is belt-and-suspenders on top of that, not a substitute
        # for it: `static` linkage would prevent the symbol from surviving
        # to the export table regardless of any attribute.
        lines.append(f"{adapter_ret_ctype} {export_name}({params_decl}) {{")
        # Gate 5's own execution-trace hook (issue #22) -- a true no-op
        # outside a -DLIND_MARSHAL_DEBUG build (see lind_marshal.h's own
        # doc on _lind_dbg_call), so emitting this call unconditionally in
        # every one of this library's generated adapters costs nothing in
        # a normal build and needs no #ifdef here.
        lines.append(f'    _lind_dbg_call("{name}");')
        lines.append("    _lind_marshal_reset();")
        lines.append("    _lind_marshal_source_cage = source_cage;")
        lines.append("    _lind_marshal_grate_cage  = grate_cage;")
        if n > 0:
            raw_inits = ", ".join(bits_expr(ct, f"raw{i}") for i, ct in enumerate(arg_ctypes))
            lines.append(f"    uint64_t raw_args[{n}] = {{ {raw_inits} }};")
            lines.append(f"    struct _lind_shadow shadows[{n}];")
        else:
            lines.append("    uint64_t *raw_args = (uint64_t *)0;")
            lines.append("    struct _lind_shadow *shadows = (struct _lind_shadow *)0;")
        lines.append("    uint32_t nshadows = 0;")
        for i in range(n):
            lines.append(
                f"    uint64_t h{i} = _lind_marshal_prepare_arg({i}, &{argspecs_name}[{i}], "
                f'raw_args, {n}, source_cage, grate_cage, shadows, &nshadows, "{name}");'
            )

        call_arg_ctypes = ["void *" if c == "uint32_t" else c for c in arg_ctypes]
        call_args = ", ".join(
            value_from_bits_expr(call_arg_ctypes[i], f"h{i}") for i in range(n)
        )
        real_call = f"{name}({call_args})"

        if is_void:
            lines.append(f"    {real_call};")
            lines.append("    uint64_t handler_ret = 0;")
        else:
            lines.append(f"    {real_ret_ctype} real_ret = {real_call};")
            if real_ret_ctype == "double":
                lines.append("    uint64_t handler_ret = _lind_v2_bits_from_f64(real_ret);")
            elif real_ret_ctype == "float":
                lines.append("    uint64_t handler_ret = _lind_v2_bits_from_f32(real_ret);")
            elif real_ret_ctype == "void *":
                lines.append("    uint64_t handler_ret = (uint64_t)(uintptr_t)real_ret;")
            else:
                lines.append("    uint64_t handler_ret = (uint64_t)real_ret;")

        if n > 0:
            lines.append("    for (uint32_t s = 0; s < nshadows; s++)")
            lines.append(
                f"        _lind_marshal_finish_shadow(s, {argspecs_name}, shadows, nshadows, "
                f"source_cage, grate_cage);"
            )
        lines.append(
            f"    uint64_t result = _lind_marshal_translate_return(&{retspec_name}, "
            f"raw_args, {n}, shadows, nshadows, handler_ret, source_cage, grate_cage);"
        )
        lines.append("    _lind_marshal_reset();")
        if ret_wasm_ctype is None:
            lines.append("    (void)result;")
        elif ret_wasm_ctype == "double":
            lines.append("    return _lind_v2_f64_from_bits(result);")
        elif ret_wasm_ctype == "float":
            lines.append("    return _lind_v2_f32_from_bits(result);")
        else:
            lines.append(f"    return ({ret_wasm_ctype})result;")
        lines.append("}")

        adapter_body = "\n".join(lines)
        return extern_decl, adapter_body, export_name, arg_ctypes, ret_wasm_ctype


V2_ADAPTER_TEMPLATE = r'''// AUTO-GENERATED by tools/marshal-gen/gen_v2_adapter.py — do not edit by hand.
// V2 (variable-width) auto-interposition adapters for {lib_name}: one
// generated function per symbol, each with its OWN exact lowered wasm
// signature (see this file's own header comment for why no fpcast-emu trick
// is needed here).
//
// Compile:
//   lind-clang -s --compile-grate {lib_name}_v2_grate.c -- -I<lind_marshal.h dir> <{lib_name}.a>
#include <lind_syscall.h>
#include <stdint.h>
#include <string.h>

#include "lind_marshal.h"

// --- real library functions (defined via static-linked {lib_name}.a) ---
{externs}

// --- per-function marshalling specs (translated from {lib_name}.marshal.json) ---
{specs}

// --- generated adapters ---
{adapters}

// V2 registration/resolution (wasmtime_lind_3i::v2_adapter) requires every
// V2 module to export this exact zero-arg, i32-returning function -- see
// MANIFEST_VERSION_EXPORT. A function, not a data global: an ordinary C
// global lives in linear memory (a data address), not as a wasm-level
// `global` value, so a real exported function is what this toolchain can
// actually give the resolver a value through without a separate memory read.
__attribute__((export_name("{manifest_version_export}")))
int {manifest_version_export}(void) {{ return {manifest_version}; }}
'''


# Self-contained V2 grate: adapters plus a generated main()/fork()/
# register_lib_handler_v2 registration loop, the V2 counterpart of
# gen_grate.py's GRATE_TEMPLATE (register_lib_handler). Used for all new
# library-interposition work; existing hand-written V1 grates keep running
# unchanged, and existing hand-written V2 grates (which compile a --only
# adapter file alongside their own main()) keep using V2_ADAPTER_TEMPLATE
# via plain --out with no --emit-grate, since a second main() here would
# collide with theirs.
GRATE_TEMPLATE_V2 = r'''// AUTO-GENERATED by tools/marshal-gen/gen_v2_adapter.py --emit-grate —
// do not edit by hand.
// Self-contained V2 (variable-width) auto-interposition grate for
// {lib_name}: registers a generated adapter for every inference-marshalable
// function via register_lib_handler_v2, with no raw-ABI-slot ceiling. The
// V2 counterpart of gen_grate.py's GRATE_TEMPLATE (register_lib_handler);
// the syscall-grate ABI (register_handler/dispatch_syscall) is a separate
// mechanism, unaffected by this transport.
//
// Compile:
//   lind-clang -s --compile-grate {lib_name}_v2_grate.c -- -I<lind_marshal.h dir> <{lib_name}.a>
// Run (from lindfs/):
//   lind-wasm --preload env=/lib/{lib_name}.so grates/{lib_name}_v2_grate.cwasm <app...>
#include <lind_syscall.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdint.h>

#include "lind_marshal.h"

// --- real library functions (defined via static-linked {lib_name}.a) ---
{externs}

// --- per-function marshalling specs (translated from {lib_name}.marshal.json) ---
{specs}

// --- generated adapters ---
{adapters}

// V2 registration/resolution requires every V2 module to export this exact
// zero-arg, i32-returning function -- see V2_ADAPTER_TEMPLATE's own comment.
__attribute__((export_name("{manifest_version_export}")))
int {manifest_version_export}(void) {{ return {manifest_version}; }}

// --compile-grate unconditionally requires a pass_fptr_to_wt export even
// when the grate registers no V1 handler -- see gen_grate.py's own
// pass_fptr_to_wt for the V1 call shape this replaces. Unreachable here.
int64_t pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {{
    (void)fn_ptr_uint; (void)cageid; (void)arg1; (void)arg1cage;
    (void)arg2; (void)arg2cage; (void)arg3; (void)arg3cage;
    (void)arg4; (void)arg4cage; (void)arg5; (void)arg5cage;
    (void)arg6; (void)arg6cage;
    fprintf(stderr, "[{lib_name}-v2-grate] FAIL: pass_fptr_to_wt reached (should be unreachable)\n");
    __builtin_trap();
}}

// --- per-symbol V2 registration table ---
struct v2_reg_entry {{ const char *name; const char *adapter; const char *sig; }};
static struct v2_reg_entry g_table[] = {{
{table}
}};
#define G_TABLE_N ((int)(sizeof(g_table)/sizeof(g_table[0])))

int main(int argc, char *argv[]) {{
    if (argc < 2) {{ fprintf(stderr, "Usage: %s <app> [args...]\n", argv[0]); __builtin_trap(); }}
    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) {{ perror("fork"); __builtin_trap(); }}
    if (pid == 0) {{
        int cageid = getpid();
        int ok = 0, fail = 0;
        for (int i = 0; i < G_TABLE_N; i++) {{
            int r = register_lib_handler_v2(cageid, "env", g_table[i].name,
                        grateid, g_table[i].adapter, g_table[i].sig);
            if (r == 0) ok++;
            else {{ fail++; fprintf(stderr, "[{lib_name}-v2-grate] register %s failed: %d\n",
                                    g_table[i].name, r); }}
        }}
        fprintf(stderr, "[{lib_name}-v2-grate] registered %d/%d handlers\n", ok, ok + fail);
        // Fail closed: same rationale as gen_grate.py's GRATE_TEMPLATE -- an
        // incomplete handler table would let some interposed calls silently
        // fall through to the app's own (uninterposed) library.
        if (fail > 0) {{
            fprintf(stderr, "[{lib_name}-v2-grate] FATAL: %d/%d handler registrations failed — aborting startup\n",
                    fail, ok + fail);
            __builtin_trap();
        }}
        if (execv(argv[1], &argv[1]) == -1) {{ perror("execv"); __builtin_trap(); }}
    }}
    int status;
    while (wait(&status) > 0) {{}}
    int ce = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    fprintf(stderr, "[{lib_name}-v2-grate] app exited %d\n", ce);
    return ce == 0 ? 0 : 1;
}}
'''


def v2_signature_desc(arg_ctypes, ret_wasm_ctype, manifest_version):
    """Builds the register_lib_handler_v2 signature descriptor string
    ("<manifest_version>:<params>:<results>") from a generated adapter's own
    wasm-level parameter/result C types -- see lib_handler_table_v2.rs's
    parse_v2_signature_desc."""
    params = "".join(V2_SIG_CHAR[c] for c in arg_ctypes)
    results = "" if ret_wasm_ctype is None else V2_SIG_CHAR[ret_wasm_ctype]
    return f"{manifest_version}:{params}:{results}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("json", help="<lib>.marshal.json")
    ap.add_argument("--lib-name", required=True, help="e.g. libz")
    ap.add_argument("--out", required=True, help="output .c file")
    ap.add_argument("--only", default="", help="comma-separated subset of function names to interpose")
    ap.add_argument("--manifest-version", type=int, default=DEFAULT_MANIFEST_VERSION)
    ap.add_argument("--emit-grate", action="store_true",
                    help="emit a self-contained grate (adapters + register_lib_handler_v2 "
                         "main()) instead of adapters alone -- see GRATE_TEMPLATE_V2")
    ap.add_argument("--allow-partial", action="store_true",
                    help="generate handlers for whatever's available even if some "
                         "requested (--only) or marshal-decision symbol is absent, "
                         "not decision=\"marshal\", or unsupported by this generator -- "
                         "every omitted symbol is still listed on stderr. Without this, "
                         "any such gap is a hard error: a partially-generated grate that "
                         "silently omits a symbol is a worse failure mode than refusing "
                         "to generate at all, since the omission surfaces only as a "
                         "missing-import/fallback-to-uninterposed bug much later, at run time.")
    args = ap.parse_args()

    d = json.load(open(args.json))
    marshal_fns = [f for f in d["functions"] if f.get("decision") == "marshal"]
    by_name = {f["name"]: f for f in d["functions"]}
    dropped = [(f["name"], v2_unmarshalable_reason(f)) for f in marshal_fns if not is_v2_marshalable(f)]
    fns = [f for f in marshal_fns if is_v2_marshalable(f)]

    if args.only:
        want = {n.strip() for n in args.only.split(",") if n.strip()}
        missing = []
        for n in sorted(want):
            f = by_name.get(n)
            if f is None:
                missing.append((n, "not present in this JSON at all"))
            elif f.get("decision") != "marshal":
                missing.append((n, f"decision={f.get('decision')!r}, not \"marshal\""))
            elif not is_v2_marshalable(f):
                missing.append((n, v2_unmarshalable_reason(f)))
        fns = [f for f in fns if f["name"] in want]
        if missing:
            if not args.allow_partial:
                print(f"[gen_v2_adapter] ERROR: {len(missing)} requested --only symbol(s) "
                      f"unavailable (pass --allow-partial to generate the rest anyway):",
                      file=sys.stderr)
                for n, reason in missing:
                    print(f"  - {n}: {reason}", file=sys.stderr)
                sys.exit(1)
            print(f"[gen_v2_adapter] --allow-partial: omitting {len(missing)} requested "
                  f"symbol(s):", file=sys.stderr)
            for n, reason in missing:
                print(f"  - {n}: {reason}", file=sys.stderr)
    elif dropped and not args.allow_partial:
        print(f"[gen_v2_adapter] ERROR: {len(dropped)} marshal-decision record(s) would be "
              f"silently omitted (pass --allow-partial to generate the rest anyway):",
              file=sys.stderr)
        for dname, reason in sorted(dropped):
            print(f"  - {dname}: {reason}", file=sys.stderr)
        sys.exit(1)

    if not fns:
        print("[gen_v2_adapter] ERROR: zero handlers would be emitted", file=sys.stderr)
        sys.exit(1)

    em = V2AdapterEmitter()
    externs, adapters, export_names, table = [], [], [], []
    for f in fns:
        extern_decl, adapter_body, export_name, arg_ctypes, ret_wasm_ctype = \
            em.emit_v2_adapter(f, args.manifest_version)
        externs.append(extern_decl)
        adapters.append(f'__attribute__((export_name("{export_name}")))\n{adapter_body}')
        export_names.append(export_name)
        if args.emit_grate:
            sig = v2_signature_desc(arg_ctypes, ret_wasm_ctype, args.manifest_version)
            table.append(f'    {{ "{f["name"]}", "{export_name}", "{sig}" }},')

    template = GRATE_TEMPLATE_V2 if args.emit_grate else V2_ADAPTER_TEMPLATE
    out = template.format(
        lib_name=args.lib_name,
        externs="\n".join(externs) if externs else "// (no externs)",
        specs="\n".join(em.decls),
        adapters="\n\n".join(adapters) if adapters else "// (no adapters)",
        manifest_version_export=MANIFEST_VERSION_EXPORT,
        manifest_version=args.manifest_version,
        table="\n".join(table),
    )
    with open(args.out, "w") as fh:
        fh.write(out)
    if args.only:
        # The relevant omissions for --only were already reported above
        # (scoped to what was actually requested); the whole-file `dropped`
        # count would otherwise report on symbols nobody asked for here.
        print(f"[gen_v2_adapter] {len(fns)} adapters ({', '.join(export_names)})")
    else:
        print(f"[gen_v2_adapter] {len(fns)} adapters ({', '.join(export_names)}), "
              f"{len(dropped)} dropped-unsupported")
        for dname, reason in sorted(dropped):
            print(f"[gen_v2_adapter] dropped: {dname}: {reason}", file=sys.stderr)
    print(f"[gen_v2_adapter] wrote {args.out}")


if __name__ == "__main__":
    main()
