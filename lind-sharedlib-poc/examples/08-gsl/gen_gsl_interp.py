#!/usr/bin/env python3
"""Generate shim + stub for GSL interpolation (Stage A): gsl_spline + gsl_interp_accel.

Builds on two mechanisms already proven:
  * opaque handles (Arg::Handle / call_handle) — the spline and the accelerator live in the
    guest; the app holds tokens.
  * the library-owned TYPE descriptor (from gen_gsl_rng.py) — gsl_spline_alloc takes a
    `const gsl_interp_type *` (gsl_interp_cspline, ...). The stub exports each descriptor name
    as a sentinel index; the shim resolves index -> the guest's real &gsl_interp_cspline.
No engine change.

We wrap the HIGH-LEVEL spline API, not the low-level gsl_interp eval family: gsl_spline bundles
the (xa, ya) data into the object at init time, so gsl_spline_eval(spline, x, accel) takes only
scalars + handles and marshals cleanly. The low-level gsl_interp_eval(interp, xa, ya, x, accel)
re-passes the data arrays on every call with an IMPLICIT length (read from the object), which
would need per-handle size tracking — deferred to a later stage.

Token kinds (C order):
  TYP            gsl_interp_type* descriptor -> i32 index (constructor only)
  SZ(name)       size_t -> Arg::USize
  SD(name)       double -> Arg::F64
  HS(name)       self handle (this module's class)      -> Arg::Handle
  HSFREE(name)   self handle, released                   -> Arg::Handle{release:true}
  HX(name,cls,ctype)  cross-class handle (the accelerator), NULLable (token 0 -> guest NULL)
  ARR(name,szn)  const double[] of length arg `szn`      -> Arg::Buf
  OD(name)       double* out (the _e forms)              -> Arg::Out[8], copied back
Returns: 'h' handle, 'd' double, 'i' int, 'u' unsigned int, 'sz' size_t, 'v' void.

Outputs:  generated/interp_accel_shim.inc.c + stub/src/interp_accel_generated.rs
          generated/spline_shim.inc.c       + stub/src/spline_generated.rs
"""
import os

CLASS_INTERP_ACCEL = 18
CLASS_SPLINE = 19

# library-owned interpolation-type descriptors (order fixes the index the shim resolves)
INTERP_TYPES = [
    "gsl_interp_linear", "gsl_interp_polynomial", "gsl_interp_cspline",
    "gsl_interp_cspline_periodic", "gsl_interp_akima", "gsl_interp_akima_periodic",
    "gsl_interp_steffen",
]

# token constructors
def TYP():                 return ("typ",)
def SZ(n):                 return ("sz", n)
def SD(n):                 return ("sd", n)
def HS(n):                 return ("hs", n)
def HSFREE(n):             return ("hsfree", n)
def HX(n, cls, ctype):     return ("hx", n, cls, ctype)
def ARR(n, szn):           return ("arr", n, szn)
def OD(n):                 return ("od", n)

ACCEL = ("CLASS_INTERP_ACCEL", "gsl_interp_accel")   # for cross-class HX in spline

RET_C = {"d": "double", "i": "int", "u": "unsigned int", "sz": "size_t", "v": "void"}


def accel_module():
    return {
        "tag": "interp_accel", "prefix": "gsl_interp_accel", "ctype": "gsl_interp_accel",
        "class_name": "CLASS_INTERP_ACCEL", "class_id": CLASS_INTERP_ACCEL,
        "type_table": None,
        "funcs": [
            ("alloc", "h",  []),
            ("reset", "i",  [HS("a")]),
            ("free",  "v",  [HSFREE("a")]),
            ("find",  "sz", [HS("a"), ARR("xa", "n"), SZ("n"), SD("x")]),
        ],
    }


def spline_module():
    return {
        "tag": "spline", "prefix": "gsl_spline", "ctype": "gsl_spline",
        "class_name": "CLASS_SPLINE", "class_id": CLASS_SPLINE,
        "type_table": INTERP_TYPES,          # gsl_spline_alloc resolves a gsl_interp_type*
        "structs": ["gsl_spline", "gsl_interp_accel", "gsl_interp_type"],
        "funcs": [
            ("alloc",         "h", [TYP(), SZ("n")]),
            ("init",          "i", [HS("spline"), ARR("xa", "n"), ARR("ya", "n"), SZ("n")]),
            ("free",          "v", [HSFREE("spline")]),
            ("min_size",      "u", [HS("spline")]),
            ("eval",          "d", [HS("spline"), SD("x"), HX("acc", *ACCEL)]),
            ("eval_e",        "i", [HS("spline"), SD("x"), HX("acc", *ACCEL), OD("y")]),
            ("eval_deriv",    "d", [HS("spline"), SD("x"), HX("acc", *ACCEL)]),
            ("eval_deriv_e",  "i", [HS("spline"), SD("x"), HX("acc", *ACCEL), OD("d")]),
            ("eval_deriv2",   "d", [HS("spline"), SD("x"), HX("acc", *ACCEL)]),
            ("eval_deriv2_e", "i", [HS("spline"), SD("x"), HX("acc", *ACCEL), OD("d2")]),
            ("eval_integ",    "d", [HS("spline"), SD("a"), SD("b"), HX("acc", *ACCEL)]),
            ("eval_integ_e",  "i", [HS("spline"), SD("a"), SD("b"), HX("acc", *ACCEL), OD("result")]),
        ],
    }

MODULES = [accel_module(), spline_module()]


# ---- C emission --------------------------------------------------------------
def c_ret(ret, ctype):
    return f"struct {ctype} *" if ret == "h" else RET_C[ret]

def c_params(ctype, params, wire):
    """wire=False: real GSL signature; wire=True: the exported lind_ signature (TYP -> int idx)."""
    out = []
    for p in params:
        k = p[0]
        if   k == "typ":    out.append("int idx" if wire else "const struct gsl_interp_type * T")
        elif k == "sz":     out.append(f"size_t {p[1]}")
        elif k == "sd":     out.append(f"double {p[1]}")
        elif k in ("hs", "hsfree"): out.append(f"struct {ctype} * {p[1]}")
        elif k == "hx":     out.append(f"struct {p[3]} * {p[1]}")
        elif k == "arr":    out.append(f"const double {p[1]}[]")
        elif k == "od":     out.append(f"double * {p[1]}")
    return ", ".join(out) or "void"

def c_call_args(params):
    out = []
    for p in params:
        if   p[0] == "typ": out.append("tbl[idx]")   # resolved from the index via the local table
        else:               out.append(p[1])
    return ", ".join(out)

def emit_c(mod):
    ctype, prefix = mod["ctype"], mod["prefix"]
    L = [f"// @generated by gen_gsl_interp.py — do not edit. #included by gsl_lind_shim.c"]
    for sc in mod.get("structs", [ctype]):
        L.append(f"struct {sc};")
    if mod["type_table"]:
        for t in mod["type_table"]:
            L.append(f"extern const struct gsl_interp_type *{t};")
    L.append("")
    for tail, ret, params in mod["funcs"]:
        name = f"{prefix}_{tail}"
        rc = c_ret(ret, ctype)
        L.append(f"extern {rc} {name}({c_params(ctype, params, wire=False)});")
        L.append(f'__attribute__((export_name("lind_{name}")))')
        has_typ = any(p[0] == "typ" for p in params)
        if has_typ:   # constructor that resolves a type descriptor index
            tbl = "{ 0, " + ", ".join(mod["type_table"]) + " }"
            L.append(f"{rc} lind_{name}({c_params(ctype, params, wire=True)}) {{")
            L.append("    // automatic (not static): the descriptors are extern pointer variables.")
            L.append(f"    const struct gsl_interp_type *tbl[] = {tbl};")
            L.append(f"    if (idx < 1 || idx > {len(mod['type_table'])}) return 0;")
            L.append(f"    return {name}({c_call_args(params)});")
            L.append("}")
        else:
            body = ("return " if ret != "v" else "") + f"{name}({c_call_args(params)})"
            L.append(f"{rc} lind_{name}({c_params(ctype, params, wire=True)}) {{ {body}; }}")
        L.append("")
    return "\n".join(L)


# ---- Rust emission -----------------------------------------------------------
def rust_ret(ret):
    return {"h": " -> *mut c_void", "d": " -> f64", "i": " -> c_int",
            "u": " -> c_uint", "sz": " -> usize", "v": ""}[ret]

def emit_rust(mod):
    cls = mod["class_name"]
    L = ["// @generated by gen_gsl_interp.py — do not edit. include!-d by stub/src/lib.rs.",
         "// Uses call/call_f64/call_handle/Arg/OutLen from lib.rs; c_int/c_uint/c_void from core::ffi."]
    if mod["class_id"]:
        L.append(f"const {cls}: u32 = {mod['class_id']};")
    L.append("")
    for tail, ret, params in mod["funcs"]:
        name = f"{mod['prefix']}_{tail}"
        rp, ae, pre, post = [], [], [], []
        has_typ = any(p[0] == "typ" for p in params)
        for p in params:
            k = p[0]
            if k == "typ":
                rp.append("t: *const c_void")
                ae.append("Arg::I32(t as usize as i64 as i32)")
            elif k == "sz":
                rp.append(f"{p[1]}: usize"); ae.append(f"Arg::USize({p[1]})")
            elif k == "sd":
                rp.append(f"{p[1]}: f64"); ae.append(f"Arg::F64({p[1]})")
            elif k == "hs":
                rp.append(f"{p[1]}: *mut c_void")
                ae.append(f"Arg::Handle {{ class: {cls}, token: {p[1]} as u64, release: false }}")
            elif k == "hsfree":
                rp.append(f"{p[1]}: *mut c_void")
                ae.append(f"Arg::Handle {{ class: {cls}, token: {p[1]} as u64, release: true }}")
            elif k == "hx":   # cross-class handle (accelerator), NULL passes through as token 0
                rp.append(f"{p[1]}: *mut c_void")
                ae.append(f"Arg::Handle {{ class: {p[2]}, token: {p[1]} as u64, release: false }}")
            elif k == "arr":
                rp.append(f"{p[1]}: *const f64")
                pre.append(f"    let {p[1]}_b = unsafe {{ core::slice::from_raw_parts({p[1]} as *const u8, ({p[2]}) * 8) }};")
                ae.append(f"Arg::Buf({p[1]}_b)")
            elif k == "od":
                rp.append(f"{p[1]}: *mut f64")
                pre.append(f"    let mut {p[1]}_o = [0u8; 8];")
                ae.append(f"Arg::Out {{ dst: &mut {p[1]}_o, len: OutLen::Cap }}")
                post.append(f"    if !{p[1]}.is_null() {{ unsafe {{ *{p[1]} = f64::from_le_bytes({p[1]}_o); }} }}")
        L.append("#[unsafe(no_mangle)]")
        L.append(f'pub extern "C" fn {name}({", ".join(rp)}){rust_ret(ret)} {{')
        L += pre
        args = ", ".join(ae)
        if ret == "h":
            L.append(f'    call_handle("lind_{name}", &mut [{args}], {cls}) as *mut c_void')
        elif ret == "d":
            L.append(f'    call_f64("lind_{name}", &mut [{args}])')
        else:
            L.append(f'    let _r = call("lind_{name}", &mut [{args}]);')
            L += post
            if   ret == "i":  L.append("    _r as c_int")
            elif ret == "u":  L.append("    _r as u32 as c_uint")
            elif ret == "sz": L.append("    _r as usize")
        L.append("}")
        L.append("")
    return "\n".join(L)


def emit_type_statics():
    """Exported descriptor sentinels — value is the index the shim resolves."""
    L = ["// library-owned gsl_interp_type descriptors — value is the index resolved in the shim."]
    for i, t in enumerate(INTERP_TYPES, start=1):
        L.append(f"#[unsafe(no_mangle)] pub static {t}: usize = {i};")
    return "\n".join(L) + "\n\n"


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    gen = os.path.join(here, "generated"); os.makedirs(gen, exist_ok=True)
    total = 0
    for mod in MODULES:
        with open(os.path.join(gen, f"{mod['tag']}_shim.inc.c"), "w") as f:
            f.write(emit_c(mod))
        rust = emit_rust(mod)
        if mod["type_table"]:          # the spline module carries the descriptor sentinels
            rust = rust.split("\n\n", 1)
            rust = rust[0] + "\n\n" + emit_type_statics() + (rust[1] if len(rust) > 1 else "")
        with open(os.path.join(here, "stub", "src", f"{mod['tag']}_generated.rs"), "w") as f:
            f.write(rust)
        print(f"  {mod['tag']}: {len(mod['funcs'])} fns (class {mod['class_id']})")
        total += len(mod["funcs"])
    print(f"generated {total} functions + {len(INTERP_TYPES)} interp-type descriptors across {len(MODULES)} modules")


if __name__ == "__main__":
    main()
