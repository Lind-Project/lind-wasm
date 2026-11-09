#!/usr/bin/env python3
"""Generate shim forwarders (C) + stub marshalling (Rust) for GSL's flat SCALAR functions.

Many whole GSL modules are "scalar math": every argument is a scalar (double / int /
unsigned int / size_t) and the result is a scalar (double / int). These need no buffers,
no handles, no callbacks — only the trivial width-preserving scalar marshalling the engine
already does. Rather than hand-listing them, we SCRAPE the GSL headers: each matching
single-line prototype is parsed, and any function whose signature is entirely scalar is
emitted automatically. Anything with a pointer, array, or opaque/struct type is SKIPPED and
reported (those belong to the buffer / handle / callback generators, not here).

Covered now (see SOURCES): cdf (cumulative distribution fns, ~86), randist `_pdf`
(distribution densities, scalar ones), sys (scalar math helpers).

Type -> token mapping (scalar only):
  double        -> F64        (Arg::F64)
  int           -> I32        (Arg::I32)
  unsigned int  -> U32        (Arg::I32, bit-preserving)
  size_t        -> USIZE      (Arg::USize)
Return: double -> call_f64 (-> f64); any other scalar -> call (-> c_int / etc.).

Outputs:
  generated/scalar_shim.inc.c    (#included by gsl_lind_shim.c)
  stub/src/scalar_generated.rs   (include!-d by stub/src/lib.rs)

The GSL source tree is found via $GSL_SRC (default: ../../../../lind-wasm-apps/gsl-2.8).
"""
import glob
import os
import re
import sys

# (header path (relative to GSL_SRC; may be a *.h glob), regex a function NAME must fully
# match, module tag)
SOURCES = [
    ("cdf/gsl_cdf.h",         r"gsl_cdf_\w+",        "cdf"),
    ("randist/gsl_randist.h", r"gsl_ran_\w+_pdf",    "randist"),
    ("sys/gsl_sys.h",         r"gsl_\w+",            "sys"),
    ("specfunc/gsl_sf_*.h",   r"gsl_sf_\w+",         "specfunc"),
]

# GSL scalar typedefs resolved to their underlying C scalar before classification, so a
# function taking e.g. gsl_mode_t still counts as fully scalar.
TYPEDEFS = {
    "gsl_mode_t": "unsigned int",   # gsl_mode.h: typedef unsigned int gsl_mode_t
}

# A prototype: <ret> <name> ( <args> ) ;   on a single logical line.
PROTO = re.compile(r"^\s*(double|int|unsigned int|size_t)\s+(\**)\s*(\w+)\s*\(([^;{}]*)\)\s*;", re.M)

# Map a (cleaned) scalar C arg type to (token, rust_param_type).
SCALAR = {
    "double":       ("f64",   "f64"),
    "int":          ("i32",   "c_int"),
    "unsigned int": ("u32",   "c_uint"),
    "unsigned":     ("u32",   "c_uint"),
    "size_t":       ("usize", "usize"),
}


class Unsupported(Exception):
    pass


def clean_type(tok):
    """Normalize one C parameter to its scalar type name, or raise Unsupported."""
    t = tok.strip()
    if not t:
        raise Unsupported("empty")
    if "*" in t or "[" in t:
        raise Unsupported("pointer/array")
    # drop a trailing identifier (the param name) and qualifiers
    t = t.replace("const", " ").replace("register", " ")
    parts = t.split()
    # If the last token looks like a param name (not a type keyword), drop it.
    typ_words = {"double", "int", "unsigned", "size_t", "long", "short", "char", "float", "void"}
    if parts and parts[-1] not in typ_words:
        parts = parts[:-1]
    name = " ".join(parts).strip()
    name = TYPEDEFS.get(name, name)   # resolve gsl_mode_t etc. to a scalar
    if name in SCALAR:
        return name
    raise Unsupported(f"non-scalar type `{name or t}`")


def parse_args(argstr):
    """Return a list of scalar type names, or raise Unsupported."""
    s = argstr.strip()
    if s in ("", "void"):
        return []
    return [clean_type(a) for a in s.split(",")]


def scrape(gsl_src):
    funcs = []          # (name, ret_type, [arg_type_names])
    skipped = []        # (name, reason)
    seen = set()
    for rel, name_rx, tag in SOURCES:
        paths = sorted(glob.glob(os.path.join(gsl_src, rel))) if any(c in rel for c in "*?[") \
                else [os.path.join(gsl_src, rel)]
        if not paths:
            print(f"  WARN: no headers matched: {rel}", file=sys.stderr)
        for path in paths:
            if not os.path.exists(path):
                print(f"  WARN: header not found: {path}", file=sys.stderr)
                continue
            _scrape_file(path, name_rx, funcs, skipped, seen)
    funcs.sort()
    return funcs, skipped


def _scrape_file(path, name_rx, funcs, skipped, seen):
    text = open(path, encoding="utf-8", errors="replace").read()
    name_re = re.compile(f"^{name_rx}$")
    for m in PROTO.finditer(text):
            ret, stars, name, args = m.group(1), m.group(2), m.group(3), m.group(4)
            if not name_re.match(name) or name in seen:
                continue
            if stars:  # pointer return
                skipped.append((name, "pointer return")); seen.add(name); continue
            try:
                atypes = parse_args(args)
            except Unsupported as e:
                skipped.append((name, str(e))); seen.add(name); continue
            seen.add(name)
            funcs.append((name, ret, atypes))


# --- emit ---------------------------------------------------------------------

def c_params(atypes):
    # name each arg a0, a1, ... (names are irrelevant to the forwarder)
    return ", ".join(f"{t} a{i}" for i, t in enumerate(atypes)) or "void"

def c_args(n):
    return ", ".join(f"a{i}" for i in range(n))

def emit_c(funcs):
    out = ["// @generated by gen_gsl_scalar.py — do not edit. #included by gsl_lind_shim.c", ""]
    for name, ret, atypes in funcs:
        out.append(f"extern {ret} {name}({c_params(atypes)});")
        out.append(f'__attribute__((export_name("lind_{name}")))')
        call = f"{name}({c_args(len(atypes))})"
        out.append(f"{ret} lind_{name}({c_params(atypes)}) {{ return {call}; }}")
        out.append("")
    return "\n".join(out)


def rust_ret(ret):
    return {"double": " -> f64", "int": " -> c_int",
            "unsigned int": " -> c_uint", "size_t": " -> usize"}[ret]

def rust_fn(name, ret, atypes):
    params = ", ".join(f"a{i}: {SCALAR[t][1]}" for i, t in enumerate(atypes))
    argexprs = []
    for i, t in enumerate(atypes):
        tok = SCALAR[t][0]
        if tok == "f64":     argexprs.append(f"Arg::F64(a{i})")
        elif tok == "i32":   argexprs.append(f"Arg::I32(a{i})")
        elif tok == "u32":   argexprs.append(f"Arg::I32(a{i} as i32)")
        elif tok == "usize": argexprs.append(f"Arg::USize(a{i})")
    args_join = ", ".join(argexprs)
    L = [f'pub extern "C" fn {name}({params}){rust_ret(ret)} {{']
    if ret == "double":
        L.append(f'    call_f64("lind_{name}", &mut [{args_join}])')
    else:
        rc = {"int": "c_int", "unsigned int": "c_uint", "size_t": "usize"}[ret]
        L.append(f'    call("lind_{name}", &mut [{args_join}]) as {rc}')
    L.append("}")
    return "#[unsafe(no_mangle)]\n" + "\n".join(L)

def emit_rust(funcs):
    out = ["// @generated by gen_gsl_scalar.py — do not edit. include!-d by stub/src/lib.rs.",
           "// Uses `call`, `call_f64`, `Arg` from lib.rs; c_int/c_uint from core::ffi.", ""]
    for name, ret, atypes in funcs:
        out.append(rust_fn(name, ret, atypes))
        out.append("")
    return "\n".join(out)


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    gsl_src = os.environ.get("GSL_SRC",
                             os.path.join(here, "..", "..", "..", "..", "lind-wasm-apps", "gsl-2.8"))
    gsl_src = os.path.abspath(gsl_src)
    funcs, skipped = scrape(gsl_src)
    gen = os.path.join(here, "generated"); os.makedirs(gen, exist_ok=True)
    with open(os.path.join(gen, "scalar_shim.inc.c"), "w") as f:
        f.write(emit_c(funcs))
    with open(os.path.join(here, "stub", "src", "scalar_generated.rs"), "w") as f:
        f.write(emit_rust(funcs))
    # report
    def bucket(n):
        if n.startswith("gsl_cdf_"): return "cdf"
        if n.startswith("gsl_ran_"): return "randist"
        if n.startswith("gsl_sf_"):  return "specfunc"
        return "sys"
    by_prefix = {}
    for name, _, _ in funcs:
        by_prefix[bucket(name)] = by_prefix.get(bucket(name), 0) + 1
    print(f"GSL_SRC = {gsl_src}")
    print(f"generated {len(funcs)} scalar functions: " +
          ", ".join(f"{k}={v}" for k, v in sorted(by_prefix.items())))
    print(f"  -> generated/scalar_shim.inc.c")
    print(f"  -> stub/src/scalar_generated.rs")
    if skipped:
        print(f"skipped {len(skipped)} (not scalar — buffer/handle/callback generators):")
        for name, reason in skipped[:20]:
            print(f"    {name}: {reason}")
        if len(skipped) > 20:
            print(f"    ... and {len(skipped) - 20} more")


if __name__ == "__main__":
    main()
