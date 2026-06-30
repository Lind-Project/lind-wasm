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


def is_marshalable(f):
    """True iff the runtime can faithfully marshal every part of this spec."""
    r = (f.get("ret") or {}).get("kind")
    if r is not None and r not in SUPPORTED_RET:
        return False  # ptr_alloc, ptr_to_static, ptr_into_cursor, ...

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
int pass_fptr_to_wt(uint64_t ctx_ptr_u64, uint64_t cageid,
                    uint64_t a1, uint64_t a1c, uint64_t a2, uint64_t a2c,
                    uint64_t a3, uint64_t a3c, uint64_t a4, uint64_t a4c,
                    uint64_t a5, uint64_t a5c, uint64_t a6, uint64_t a6c) {{
    if (ctx_ptr_u64 == 0) {{ fprintf(stderr, "[{lib_name}-grate] null ctx\n"); assert(0); }}
    struct libctx *ctx = (struct libctx *)(uintptr_t)ctx_ptr_u64;
    uint64_t raw[6]   = {{ a1, a2, a3, a4, a5, a6 }};
    uint64_t cages[6] = {{ a1c, a2c, a3c, a4c, a5c, a6c }};
    uint64_t src = 0;
    for (int i = 0; i < 6 && src == 0; i++) src = cages[i];
    return (int)lind_marshal_dispatch(ctx->real_fn, ctx->spec, src, cageid,
                                      raw, ctx->spec->nargs);
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


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("json", help="<lib>.marshal.json")
    ap.add_argument("--lib-name", required=True, help="e.g. libz")
    ap.add_argument("--out", required=True, help="output .c file")
    ap.add_argument("--only", default="", help="comma-separated subset of function names to interpose")
    ap.add_argument("--no-externs", action="store_true",
                    help="don't emit extern decls (real fns come from included headers, e.g. libc)")
    ap.add_argument("--include", default="", help="comma-separated extra headers to #include")
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
    for f in fns:
        name = f["name"]
        if not args.no_externs:
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

    out = GRATE_TEMPLATE.format(
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
