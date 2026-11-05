#!/usr/bin/env python3
"""Generate shim forwarders (C) + stub marshalling (Rust) for GSL's `statistics` module.

Statistics is far more varied than sort, so instead of fixed shapes we describe each
function by an ordered list of typed PARAMETER TOKENS and emit both C and Rust from them.
This is the generator approach extended to a richer module.

Scope of this first cut (correct-by-construction, no test loop available):
  * the `double` element type only — statistics always RETURNS double (or size_t) regardless
    of input type, and the double symbols are the ones the double test.c exercises. Other
    element types are a one-loop extension (add a TYPES table) once this validates.
  * DEFERRED: weighted functions (w*, defined only for float types) and work-array functions
    (mad/mad0/Sn/Qn/spearman — need a caller-sized scratch buffer). Both are clean follow-ups.

Parameter tokens (the C parameter order is exactly the token order):
  A(name, stride, n)    const input array; span = 1+(cnt-1)*strd elems -> Arg::Buf
  AIO(name, stride, n)  in/out array (sorted in place) -> Arg::InOut
  SD(name)              scalar double in -> Arg::F64
  SZ(name)              scalar size_t in -> Arg::USize   (also named by A tokens as stride/n)
  OD(name)              double*  single output -> Arg::Out[8], written back via *name
  OI(name)              size_t*  single output -> Arg::Out[4], widened guest(4B)->host(8B)
Return: 'd' (double), 'sz' (size_t), 'v' (void).

Outputs:
  generated/stats_shim.inc.c     (#included by gsl_lind_shim.c)
  stub/src/stats_generated.rs    (include!-d by stub/src/lib.rs)
"""
import os

ESIZE = 8          # double element bytes (this cut is double-only)
GUEST_SIZE_T = 4   # wasm32 size_t width


def A(name, stride, n):  return ("arr",   name, stride, n)
def AIO(name, stride, n):return ("arrio", name, stride, n)
def SD(name):            return ("sd",    name)
def SZ(name):            return ("sz",    name)
def OD(name):            return ("od",    name)
def OI(name):            return ("oi",    name)

# (tail, return, [params in C order])
FUNCS = []

# --- reductions: f(const data[], stride, n) -> double ---
for tail in ["mean", "variance", "sd", "tss", "absdev", "skew", "kurtosis", "max", "min",
             "lag1_autocorrelation", "median_from_sorted_data", "gastwirth_from_sorted_data"]:
    FUNCS.append((tail, "d", [A("data", "stride", "n"), SZ("stride"), SZ("n")]))

# --- reductions with a given mean: f(data, stride, n, mean) -> double ---
for tail in ["variance_m", "sd_m", "variance_with_fixed_mean", "sd_with_fixed_mean",
             "tss_m", "absdev_m", "lag1_autocorrelation_m"]:
    FUNCS.append((tail, "d", [A("data", "stride", "n"), SZ("stride"), SZ("n"), SD("mean")]))

# --- with mean and sd: f(data, stride, n, mean, sd) -> double ---
for tail in ["kurtosis_m_sd", "skew_m_sd"]:
    FUNCS.append((tail, "d", [A("data", "stride", "n"), SZ("stride"), SZ("n"), SD("mean"), SD("sd")]))

# --- index-returning reductions: f(data, stride, n) -> size_t ---
for tail in ["max_index", "min_index"]:
    FUNCS.append((tail, "sz", [A("data", "stride", "n"), SZ("stride"), SZ("n")]))

# --- minmax: void f(double* min, double* max, data, stride, n) ---
FUNCS.append(("minmax", "v",
              [OD("min"), OD("max"), A("data", "stride", "n"), SZ("stride"), SZ("n")]))
# --- minmax_index: void f(size_t* min_i, size_t* max_i, data, stride, n) ---
FUNCS.append(("minmax_index", "v",
              [OI("min_index"), OI("max_index"), A("data", "stride", "n"), SZ("stride"), SZ("n")]))

# --- two arrays, shared n: f(d1, s1, d2, s2, n) -> double ---
for tail in ["covariance", "correlation"]:
    FUNCS.append((tail, "d", [A("data1", "stride1", "n"), SZ("stride1"),
                              A("data2", "stride2", "n"), SZ("stride2"), SZ("n")]))
# covariance_m: + mean1, mean2
FUNCS.append(("covariance_m", "d", [A("data1", "stride1", "n"), SZ("stride1"),
                                    A("data2", "stride2", "n"), SZ("stride2"), SZ("n"),
                                    SD("mean1"), SD("mean2")]))

# --- two arrays, own n each: f(d1, s1, n1, d2, s2, n2) -> double ---
for tail in ["pvariance", "ttest"]:
    FUNCS.append((tail, "d", [A("data1", "stride1", "n1"), SZ("stride1"), SZ("n1"),
                              A("data2", "stride2", "n2"), SZ("stride2"), SZ("n2")]))

# --- quantile: f(sorted, stride, n, f) -> double ---
FUNCS.append(("quantile_from_sorted_data", "d",
              [A("sorted_data", "stride", "n"), SZ("stride"), SZ("n"), SD("f")]))
# --- trmean: f(trim, sorted, stride, n) -> double  (scalar FIRST) ---
FUNCS.append(("trmean_from_sorted_data", "d",
              [SD("trim"), A("sorted_data", "stride", "n"), SZ("stride"), SZ("n")]))
# --- select: f(data[], stride, n, k) -> double  (data modified in place) ---
FUNCS.append(("select", "d",
              [AIO("data", "stride", "n"), SZ("stride"), SZ("n"), SZ("k")]))
# --- median: f(sorted_data[], stride, n) -> double  (non-const) ---
FUNCS.append(("median", "d", [AIO("sorted_data", "stride", "n"), SZ("stride"), SZ("n")]))

DEFERRED = ["weighted w* (float-only)", "work-array: mad, mad0, Sn, Sn0, Qn, Qn0, spearman"]

RET_C = {"d": "double", "sz": "size_t", "v": "void"}


def c_params(params):
    out = []
    for p in params:
        k = p[0]
        if k == "arr":   out.append(f"const double {p[1]}[]")
        elif k == "arrio": out.append(f"double {p[1]}[]")
        elif k == "sd":  out.append(f"const double {p[1]}")
        elif k == "sz":  out.append(f"const size_t {p[1]}")
        elif k == "od":  out.append(f"double * {p[1]}")
        elif k == "oi":  out.append(f"size_t * {p[1]}")
    return ", ".join(out)


def c_args(params):
    return ", ".join(p[1] for p in params)


def emit_c():
    out = ["// @generated by gen_gsl_stats.py — do not edit. #included by gsl_lind_shim.c", ""]
    for tail, ret, params in FUNCS:
        name = f"gsl_stats_{tail}"
        rc = RET_C[ret]
        out.append(f"extern {rc} {name}({c_params(params)});")
        out.append(f'__attribute__((export_name("lind_{name}")))')
        body = f"{name}({c_args(params)})"
        out.append(f"{rc} lind_{name}({c_params(params)}) {{ {'return ' if ret!='v' else ''}{body}; }}")
        out.append("")
    return "\n".join(out)


def rust_params(params):
    out = []
    for p in params:
        k = p[0]
        if k in ("arr",):   out.append(f"{p[1]}: *const c_void")
        elif k == "arrio":  out.append(f"{p[1]}: *mut c_void")
        elif k == "sd":     out.append(f"{p[1]}: f64")
        elif k == "sz":     out.append(f"{p[1]}: usize")
        elif k == "od":     out.append(f"{p[1]}: *mut f64")
        elif k == "oi":     out.append(f"{p[1]}: *mut usize")
    return ", ".join(out)


def rust_fn(tail, ret, params):
    name = f"gsl_stats_{tail}"
    rc = {"d": " -> f64", "sz": " -> usize", "v": ""}[ret]
    L = [f'pub extern "C" fn {name}({rust_params(params)}){rc} {{']
    argexprs = []       # the Arg::... entries, in order
    pre = []            # buffer setup before the call
    post = []           # output copy-back after the call
    oidx = 0
    for p in params:
        k = p[0]
        if k == "arr":
            strd, cnt = p[2], p[3]
            pre.append(f"    let {p[1]}_b = unsafe {{ core::slice::from_raw_parts({p[1]} as *const u8, (if {cnt} == 0 {{ 0 }} else {{ 1 + ({cnt} - 1) * {strd} }}) * {ESIZE}) }};")
            argexprs.append(f"Arg::Buf({p[1]}_b)")
        elif k == "arrio":
            strd, cnt = p[2], p[3]
            pre.append(f"    let {p[1]}_b = unsafe {{ core::slice::from_raw_parts_mut({p[1]} as *mut u8, (if {cnt} == 0 {{ 0 }} else {{ 1 + ({cnt} - 1) * {strd} }}) * {ESIZE}) }};")
            argexprs.append(f"Arg::InOut {{ dst: {p[1]}_b, len: OutLen::Cap }}")
        elif k == "sd":
            argexprs.append(f"Arg::F64({p[1]})")
        elif k == "sz":
            argexprs.append(f"Arg::USize({p[1]})")
        elif k == "od":
            pre.append(f"    let mut {p[1]}_o = [0u8; 8];")
            argexprs.append(f"Arg::Out {{ dst: &mut {p[1]}_o, len: OutLen::Cap }}")
            post.append(f"    if !{p[1]}.is_null() {{ unsafe {{ *{p[1]} = f64::from_le_bytes({p[1]}_o); }} }}")
        elif k == "oi":
            pre.append(f"    let mut {p[1]}_o = [0u8; {GUEST_SIZE_T}];")
            argexprs.append(f"Arg::Out {{ dst: &mut {p[1]}_o, len: OutLen::Cap }}")
            post.append(f"    if !{p[1]}.is_null() {{ unsafe {{ *{p[1]} = u32::from_le_bytes({p[1]}_o) as usize; }} }}")
        oidx += 1
    L += pre
    args_join = ", ".join(argexprs)
    callfn = {"d": "call_f64", "sz": "call", "v": "call"}[ret]
    call_line = f'    let _r = {callfn}("lind_{name}", &mut [{args_join}]);'
    L.append(call_line)
    L += post
    if ret == "d":
        L.append("    _r")
    elif ret == "sz":
        L.append("    _r as usize")
    # void: nothing
    L.append("}")
    return "#[unsafe(no_mangle)]\n" + "\n".join(L)


def emit_rust():
    out = ["// @generated by gen_gsl_stats.py — do not edit. include!-d by stub/src/lib.rs.",
           "// Uses `call`, `call_f64`, `Arg`, `OutLen` from lib.rs.", ""]
    for tail, ret, params in FUNCS:
        out.append(rust_fn(tail, ret, params))
        out.append("")
    return "\n".join(out)


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    gen = os.path.join(here, "generated"); os.makedirs(gen, exist_ok=True)
    with open(os.path.join(gen, "stats_shim.inc.c"), "w") as f:
        f.write(emit_c())
    with open(os.path.join(here, "stub", "src", "stats_generated.rs"), "w") as f:
        f.write(emit_rust())
    print(f"generated {len(FUNCS)} statistics functions (double type)")
    print(f"  deferred: {'; '.join(DEFERRED)}")
    print("  -> generated/stats_shim.inc.c")
    print("  -> stub/src/stats_generated.rs")


if __name__ == "__main__":
    main()
