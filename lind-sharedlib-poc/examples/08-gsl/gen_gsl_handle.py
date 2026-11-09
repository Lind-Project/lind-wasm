#!/usr/bin/env python3
"""Generate shim + stub for GSL OPAQUE-HANDLE modules (Tier 2).

Generalizes the hand-written gsl_vector handle slice. A module has an opaque object
(gsl_matrix, gsl_permutation, a workspace, ...) that lives in guest memory. A CONSTRUCTOR
returns the object's guest offset, registered by the engine as an app-facing token
(call_handle). Every other function takes that token; the engine translates it to the
guest offset (Arg::Handle). A DESTRUCTOR releases it. The object is never copied.

Container modules (matrix/vector/block) are ELEMENT-TYPED: the same shape is emitted for
double/float/int/uint. Element-typed slots use EV / 'e' / OEV, which the generator fills
from the module's `elem` config. (long/ulong/complex are excluded: long differs 8B host vs
4B guest, complex has a different API.)

Parameter tokens (C order):
  HRET          constructor return (call_handle)
  HFREE(name)   destructor handle   (Arg::Handle{release:true})
  HIN(name)     input handle        (Arg::Handle)
  SZ(name)      size_t   -> Arg::USize
  EV(name)      element value (double/float/int/uint) -> Arg::F64/F32/I32
  OI(name)      size_t*  output -> Arg::Out[4], widened 4B->host
  OEV(name)     element* output -> Arg::Out[bytes], written back as the element type
Return: 'h' handle, 'e' element, 'i' int, 'sz' size_t, 'v' void.

Outputs (per module):  generated/<tag>_shim.inc.c  +  stub/src/<tag>_generated.rs
"""
import os

GUEST_SIZE_T = 4

def HRET():      return ("hret",)
def HFREE(name): return ("hfree", name)
def HIN(name):   return ("hin",  name, None, None)         # container: class/ctype from the module
def SZ(name):    return ("sz",   name)
def EV(name):    return ("ev",   name)
def SI(name):    return ("si",   name)                     # int scalar in
def SD(name):    return ("sd",   name)                     # double scalar in
def A(name, sz): return ("a",    name, sz)                 # const double[] in, length = arg `sz`
def OI(name):    return ("oi",   name)                     # size_t* out
def OII(name):   return ("oii",  name)                     # int* out
def OD(name):    return ("od",   name)                     # double* out
def OEV(name):   return ("oev",  name)
# cross-class handle-in helpers for free-function modules (explicit class + C struct type)
def HMAT(name):  return ("hin", name, "CLASS_MATRIX",      "gsl_matrix")
def HPERM(name): return ("hin", name, "CLASS_PERMUTATION", "gsl_permutation")
def HVEC(name):  return ("hin", name, "CLASS_VECTOR",      "gsl_vector")
def HW(name, cls, ctype): return ("hin", name, cls, ctype)   # workspace / any explicit-class handle

RET_C = {"i": "int", "sz": "size_t", "v": "void", "d": "double"}

# element-type configs for container modules
ELEMS = {
    "double": {"suffix": "",       "ctype": "double",       "rust": "f64",    "bytes": 8,
               "arg": lambda n: f"Arg::F64({n})",          "callf": "call_f64", "fromle": "f64", "retcast": None},
    "float":  {"suffix": "_float",  "ctype": "float",        "rust": "f32",    "bytes": 4,
               "arg": lambda n: f"Arg::F32({n})",          "callf": "call_f32", "fromle": "f32", "retcast": None},
    "int":    {"suffix": "_int",    "ctype": "int",          "rust": "c_int",  "bytes": 4,
               "arg": lambda n: f"Arg::I32({n})",          "callf": "call",     "fromle": "i32", "retcast": "c_int"},
    "uint":   {"suffix": "_uint",   "ctype": "unsigned int", "rust": "c_uint", "bytes": 4,
               "arg": lambda n: f"Arg::I32({n} as i32)",   "callf": "call",     "fromle": "u32", "retcast": "c_uint"},
}


# ---- matrix shape (element-typed) --------------------------------------------
def matrix_shape():
    F = []
    F.append(("alloc",  "h", [SZ("n1"), SZ("n2")]))
    F.append(("calloc", "h", [SZ("n1"), SZ("n2")]))
    F.append(("free",   "v", [HFREE("m")]))
    F.append(("get",    "e", [HIN("m"), SZ("i"), SZ("j")]))
    F.append(("set",    "v", [HIN("m"), SZ("i"), SZ("j"), EV("x")]))
    F.append(("set_all",      "v", [HIN("m"), EV("x")]))
    F.append(("set_zero",     "v", [HIN("m")]))
    F.append(("set_identity", "v", [HIN("m")]))
    for tail in ["memcpy", "transpose_memcpy", "add", "sub", "mul_elements", "div_elements"]:
        F.append((tail, "i", [HIN("dest"), HIN("src")]))
    F.append(("scale",        "i", [HIN("m"), EV("x")]))
    F.append(("add_constant", "i", [HIN("m"), EV("x")]))
    F.append(("transpose",    "i", [HIN("m")]))
    F.append(("swap_rows",    "i", [HIN("m"), SZ("i"), SZ("j")]))
    F.append(("swap_columns", "i", [HIN("m"), SZ("i"), SZ("j")]))
    F.append(("equal",        "i", [HIN("a"), HIN("b")]))
    for tail in ["isnull", "ispos", "isneg", "isnonneg"]:
        F.append((tail, "i", [HIN("m")]))
    F.append(("max", "e", [HIN("m")]))
    F.append(("min", "e", [HIN("m")]))
    F.append(("minmax",       "v", [HIN("m"), OEV("min_out"), OEV("max_out")]))
    F.append(("max_index",    "v", [HIN("m"), OI("imax"), OI("jmax")]))
    F.append(("min_index",    "v", [HIN("m"), OI("imin"), OI("jmin")]))
    F.append(("minmax_index", "v", [HIN("m"), OI("imin"), OI("jmin"), OI("imax"), OI("jmax")]))
    return F


def matrix_module(elemkey, class_id):
    e = ELEMS[elemkey]
    return {"tag": f"matrix{e['suffix']}", "prefix": f"gsl_matrix{e['suffix']}",
            "ctype": f"gsl_matrix{e['suffix']}", "class_name": f"CLASS_MATRIX{e['suffix'].upper()}",
            "class_id": class_id, "elem": e, "funcs": matrix_shape()}


# ---- vector shape (element-typed, 1 index) -----------------------------------
def vector_shape():
    F = []
    F.append(("alloc",  "h", [SZ("n")]))
    F.append(("calloc", "h", [SZ("n")]))
    F.append(("free",   "v", [HFREE("v")]))
    F.append(("get",    "e", [HIN("v"), SZ("i")]))
    F.append(("set",    "v", [HIN("v"), SZ("i"), EV("x")]))
    F.append(("set_all",  "v", [HIN("v"), EV("x")]))
    F.append(("set_zero", "v", [HIN("v")]))
    F.append(("set_basis", "i", [HIN("v"), SZ("i")]))
    for tail in ["memcpy", "swap", "add", "sub", "mul", "div"]:
        F.append((tail, "i", [HIN("a"), HIN("b")]))
    F.append(("scale",        "i", [HIN("v"), EV("x")]))
    F.append(("add_constant", "i", [HIN("v"), EV("x")]))
    F.append(("swap_elements", "i", [HIN("v"), SZ("i"), SZ("j")]))
    F.append(("reverse",       "i", [HIN("v")]))
    F.append(("equal",         "i", [HIN("u"), HIN("v")]))
    for tail in ["isnull", "ispos", "isneg", "isnonneg"]:
        F.append((tail, "i", [HIN("v")]))
    F.append(("max", "e", [HIN("v")]))
    F.append(("min", "e", [HIN("v")]))
    F.append(("minmax",       "v", [HIN("v"), OEV("min_out"), OEV("max_out")]))
    F.append(("max_index",    "sz", [HIN("v")]))
    F.append(("min_index",    "sz", [HIN("v")]))
    F.append(("minmax_index", "v", [HIN("v"), OI("imin"), OI("imax")]))
    return F


def vector_module(elemkey, class_id):
    e = ELEMS[elemkey]
    return {"tag": f"vector{e['suffix']}", "prefix": f"gsl_vector{e['suffix']}",
            "ctype": f"gsl_vector{e['suffix']}", "class_name": f"CLASS_VECTOR{e['suffix'].upper()}",
            "class_id": class_id, "elem": e, "funcs": vector_shape()}


# ---- permutation / combination (no element type) -----------------------------
def permutation_funcs():
    F = [("alloc", "h", [SZ("n")]), ("calloc", "h", [SZ("n")]), ("init", "v", [HIN("p")]),
         ("free", "v", [HFREE("p")]), ("get", "sz", [HIN("p"), SZ("i")]),
         ("memcpy", "i", [HIN("dest"), HIN("src")]), ("size", "sz", [HIN("p")]),
         ("swap", "i", [HIN("p"), SZ("i"), SZ("j")]), ("valid", "i", [HIN("p")]),
         ("reverse", "v", [HIN("p")]), ("inverse", "i", [HIN("inv"), HIN("p")]),
         ("next", "i", [HIN("p")]), ("prev", "i", [HIN("p")]),
         ("mul", "i", [HIN("p"), HIN("pa"), HIN("pb")]),
         ("linear_to_canonical", "i", [HIN("q"), HIN("p")]),
         ("canonical_to_linear", "i", [HIN("p"), HIN("q")]),
         ("inversions", "sz", [HIN("p")]), ("linear_cycles", "sz", [HIN("p")]),
         ("canonical_cycles", "sz", [HIN("q")])]
    return F

def combination_funcs():
    F = [("alloc", "h", [SZ("n"), SZ("k")]), ("calloc", "h", [SZ("n"), SZ("k")]),
         ("init_first", "v", [HIN("c")]), ("init_last", "v", [HIN("c")]),
         ("free", "v", [HFREE("c")]), ("get", "sz", [HIN("c"), SZ("i")]),
         ("memcpy", "i", [HIN("dest"), HIN("src")]), ("n", "sz", [HIN("c")]),
         ("k", "sz", [HIN("c")]), ("valid", "i", [HIN("c")]),
         ("next", "i", [HIN("c")]), ("prev", "i", [HIN("c")])]
    return F


# ---- histogram (single class, always-double; arrays + double outputs) --------
def histogram_funcs():
    F = [("alloc", "h", [SZ("n")]), ("calloc", "h", [SZ("n")]), ("free", "v", [HFREE("h")]),
         ("increment",  "i", [HIN("h"), SD("x")]),
         ("accumulate", "i", [HIN("h"), SD("x"), SD("weight")]),
         ("find",       "i", [HIN("h"), SD("x"), OI("i")]),
         ("get",        "d", [HIN("h"), SZ("i")]),
         ("get_range",  "i", [HIN("h"), SZ("i"), OD("lower"), OD("upper")]),
         ("max", "d", [HIN("h")]), ("min", "d", [HIN("h")]), ("bins", "sz", [HIN("h")]),
         ("reset", "v", [HIN("h")]),
         ("set_ranges",         "i", [HIN("h"), A("range", "size"), SZ("size")]),
         ("set_ranges_uniform", "i", [HIN("h"), SD("xmin"), SD("xmax")]),
         ("max_val", "d", [HIN("h")]), ("max_bin", "sz", [HIN("h")]),
         ("min_val", "d", [HIN("h")]), ("min_bin", "sz", [HIN("h")]),
         ("sigma", "d", [HIN("h")]), ("mean", "d", [HIN("h")]), ("sum", "d", [HIN("h")]),
         ("memcpy", "i", [HIN("dest"), HIN("source")]),
         ("add", "i", [HIN("h1"), HIN("h2")]), ("sub", "i", [HIN("h1"), HIN("h2")]),
         ("scale", "i", [HIN("h"), SD("scale")]), ("shift", "i", [HIN("h"), SD("shift")]),
         ("equal_bins_p", "i", [HIN("h1"), HIN("h2")])]
    return F


# ---- eigen (workspace handles + free-function solvers) -----------------------
def workspace_module(tag, prefix, ctype, class_name, class_id, alloc_args=None):
    """A minimal opaque-workspace handle module: alloc(<sizes>) + free."""
    return {"tag": tag, "prefix": prefix, "ctype": ctype, "class_name": class_name,
            "class_id": class_id, "elem": None,
            "funcs": [("alloc", "h", alloc_args or [SZ("n")]), ("free", "v", [HFREE("w")])]}

def eigen_funcs():
    SYMM_WS  = ("CLASS_EIGEN_SYMM_WS",  "gsl_eigen_symm_workspace")
    SYMMV_WS = ("CLASS_EIGEN_SYMMV_WS", "gsl_eigen_symmv_workspace")
    return [
        ("symm",  "i", [HMAT("A"), HVEC("eval"), HW("w", *SYMM_WS)]),
        ("symmv", "i", [HMAT("A"), HVEC("eval"), HMAT("evec"), HW("w", *SYMMV_WS)]),
        ("symmv_sort", "i", [HVEC("eval"), HMAT("evec"), SI("sort_type")]),
    ]


# ---- multifit linear (workspace + least-squares solver) ----------------------
def multifit_funcs():
    WS = ("CLASS_MULTIFIT_LINEAR_WS", "gsl_multifit_linear_workspace")
    return [
        ("linear", "i", [HMAT("X"), HVEC("y"), HVEC("c"), HMAT("cov"), OD("chisq"), HW("work", *WS)]),
        ("linear_est", "i", [HVEC("x"), HVEC("c"), HMAT("cov"), OD("y"), OD("y_err")]),
    ]


# ---- linalg (free functions over matrix/permutation/vector handles) ----------
def linalg_funcs():
    F = [("LU_decomp", "i", [HMAT("A"), HPERM("p"), OII("signum")]),
         ("LU_solve",  "i", [HMAT("LU"), HPERM("p"), HVEC("b"), HVEC("x")]),
         ("LU_svx",    "i", [HMAT("LU"), HPERM("p"), HVEC("x")]),
         ("LU_invert", "i", [HMAT("LU"), HPERM("p"), HMAT("inverse")]),
         ("LU_det",    "d", [HMAT("LU"), SI("signum")]),
         ("LU_lndet",  "d", [HMAT("LU")]),
         ("LU_sgndet", "i", [HMAT("lu"), SI("signum")]),
         ("cholesky_decomp", "i", [HMAT("A")]),
         ("cholesky_solve",  "i", [HMAT("cholesky"), HVEC("b"), HVEC("x")]),
         ("cholesky_svx",    "i", [HMAT("cholesky"), HVEC("x")]),
         ("cholesky_invert", "i", [HMAT("cholesky")]),
         # QR
         ("QR_decomp",  "i", [HMAT("A"), HVEC("tau")]),
         ("QR_solve",   "i", [HMAT("QR"), HVEC("tau"), HVEC("b"), HVEC("x")]),
         ("QR_svx",     "i", [HMAT("QR"), HVEC("tau"), HVEC("x")]),
         ("QR_lssolve", "i", [HMAT("QR"), HVEC("tau"), HVEC("b"), HVEC("x"), HVEC("residual")]),
         ("QR_QTvec",   "i", [HMAT("QR"), HVEC("tau"), HVEC("v")]),
         ("QR_unpack",  "i", [HMAT("QR"), HVEC("tau"), HMAT("Q"), HMAT("R")]),
         # SVD
         ("SV_decomp", "i", [HMAT("A"), HMAT("V"), HVEC("S"), HVEC("work")]),
         ("SV_solve",  "i", [HMAT("U"), HMAT("Q"), HVEC("S"), HVEC("b"), HVEC("x")])]
    return F


MODULES = [
    vector_module("double", 1),
    vector_module("float",  8),
    vector_module("int",    9),
    vector_module("uint",   10),
    matrix_module("double", 2),
    matrix_module("float",  5),
    matrix_module("int",    6),
    matrix_module("uint",   7),
    {"tag": "permutation", "prefix": "gsl_permutation", "ctype": "gsl_permutation",
     "class_name": "CLASS_PERMUTATION", "class_id": 3, "elem": None, "funcs": permutation_funcs()},
    {"tag": "combination", "prefix": "gsl_combination", "ctype": "gsl_combination",
     "class_name": "CLASS_COMBINATION", "class_id": 4, "elem": None, "funcs": combination_funcs()},
    {"tag": "histogram", "prefix": "gsl_histogram", "ctype": "gsl_histogram",
     "class_name": "CLASS_HISTOGRAM", "class_id": 13, "elem": None, "funcs": histogram_funcs()},
    {"tag": "linalg", "prefix": "gsl_linalg", "ctype": None, "class_name": None, "elem": None,
     "structs": ["gsl_matrix", "gsl_permutation", "gsl_vector"], "funcs": linalg_funcs()},
    workspace_module("eigen_symm_ws", "gsl_eigen_symm", "gsl_eigen_symm_workspace",
                     "CLASS_EIGEN_SYMM_WS", 14),
    workspace_module("eigen_symmv_ws", "gsl_eigen_symmv", "gsl_eigen_symmv_workspace",
                     "CLASS_EIGEN_SYMMV_WS", 15),
    {"tag": "eigen", "prefix": "gsl_eigen", "ctype": None, "class_name": None, "elem": None,
     "structs": ["gsl_matrix", "gsl_vector", "gsl_eigen_symm_workspace", "gsl_eigen_symmv_workspace"],
     "funcs": eigen_funcs()},
    workspace_module("multifit_linear_ws", "gsl_multifit_linear", "gsl_multifit_linear_workspace",
                     "CLASS_MULTIFIT_LINEAR_WS", 16, [SZ("n"), SZ("p")]),
    {"tag": "multifit", "prefix": "gsl_multifit", "ctype": None, "class_name": None, "elem": None,
     "structs": ["gsl_matrix", "gsl_vector", "gsl_multifit_linear_workspace"],
     "funcs": multifit_funcs()},
]


# ---- emit --------------------------------------------------------------------
def c_ret(ret, ctype, elem):
    if ret == "h": return f"struct {ctype} *"
    if ret == "e": return elem["ctype"]
    return RET_C[ret]

def c_params(ctype, elem, params):
    out = []
    for p in params:
        k = p[0]
        if k == "hfree": out.append(f"struct {ctype} * {p[1]}")
        elif k == "hin": out.append(f"struct {p[3] or ctype} * {p[1]}")   # per-token struct for cross-class
        elif k == "sz":  out.append(f"const size_t {p[1]}")
        elif k == "si":  out.append(f"const int {p[1]}")
        elif k == "sd":  out.append(f"const double {p[1]}")
        elif k == "a":   out.append(f"const double {p[1]}[]")
        elif k == "ev":  out.append(f"const {elem['ctype']} {p[1]}")
        elif k == "oi":  out.append(f"size_t * {p[1]}")
        elif k == "oii": out.append(f"int * {p[1]}")
        elif k == "od":  out.append(f"double * {p[1]}")
        elif k == "oev": out.append(f"{elem['ctype']} * {p[1]}")
    return ", ".join(out) or "void"

def c_args(params):
    return ", ".join(p[1] for p in params if p[0] != "hret")

def emit_c(mod):
    ctype, prefix, elem = mod["ctype"], mod["prefix"], mod["elem"]
    out = ["// @generated by gen_gsl_handle.py — do not edit. #included by gsl_lind_shim.c"]
    # forward-declare the opaque struct(s) this module uses (incomplete types, pointer-only)
    for sc in ([ctype] if ctype else mod.get("structs", [])):
        out.append(f"struct {sc};")
    out.append("")
    for tail, ret, params in mod["funcs"]:
        name = f"{prefix}_{tail}"
        rc = c_ret(ret, ctype, elem)
        out.append(f"extern {rc} {name}({c_params(ctype, elem, params)});")
        out.append(f'__attribute__((export_name("lind_{name}")))')
        body = ("return " if ret != "v" else "") + f"{name}({c_args(params)})"
        out.append(f"{rc} lind_{name}({c_params(ctype, elem, params)}) {{ {body}; }}")
        out.append("")
    return "\n".join(out)


def rust_ret(ret, elem):
    if ret == "h": return " -> *mut c_void"
    if ret == "e": return f" -> {elem['rust']}"
    return {"i": " -> c_int", "sz": " -> usize", "d": " -> f64", "v": ""}[ret]

def rust_params(elem, params):
    out = []
    for p in params:
        k = p[0]
        if k in ("hfree", "hin"): out.append(f"{p[1]}: *mut c_void")
        elif k == "sz":  out.append(f"{p[1]}: usize")
        elif k == "si":  out.append(f"{p[1]}: c_int")
        elif k == "sd":  out.append(f"{p[1]}: f64")
        elif k == "a":   out.append(f"{p[1]}: *const f64")
        elif k == "ev":  out.append(f"{p[1]}: {elem['rust']}")
        elif k == "oi":  out.append(f"{p[1]}: *mut usize")
        elif k == "oii": out.append(f"{p[1]}: *mut c_int")
        elif k == "od":  out.append(f"{p[1]}: *mut f64")
        elif k == "oev": out.append(f"{p[1]}: *mut {elem['rust']}")
    return ", ".join(out)

def rust_fn(mod, tail, ret, params):
    cls, elem = mod["class_name"], mod["elem"]
    name = f"{mod['prefix']}_{tail}"
    L = [f'pub extern "C" fn {name}({rust_params(elem, params)}){rust_ret(ret, elem)} {{']
    if ret == "h":   # constructor: scalars/elements in, returns a freshly registered handle
        ae = [f"Arg::USize({p[1]})" for p in params if p[0] == "sz"]
        ae += [elem["arg"](p[1]) for p in params if p[0] == "ev"]
        L.append(f'    call_handle("lind_{name}", &mut [{", ".join(ae)}], {cls}) as *mut c_void')
        L.append("}")
        return "#[unsafe(no_mangle)]\n" + "\n".join(L)
    pre, post, ae = [], [], []
    for p in params:
        k = p[0]
        if k == "hfree":   ae.append(f"Arg::Handle {{ class: {cls}, token: {p[1]} as u64, release: true }}")
        elif k == "hin":   ae.append(f"Arg::Handle {{ class: {p[2] or cls}, token: {p[1]} as u64, release: false }}")
        elif k == "sz":    ae.append(f"Arg::USize({p[1]})")
        elif k == "si":    ae.append(f"Arg::I32({p[1]})")
        elif k == "sd":    ae.append(f"Arg::F64({p[1]})")
        elif k == "ev":    ae.append(elem["arg"](p[1]))
        elif k == "a":
            pre.append(f"    let {p[1]}_b = unsafe {{ core::slice::from_raw_parts({p[1]} as *const u8, ({p[2]}) * 8) }};")
            ae.append(f"Arg::Buf({p[1]}_b)")
        elif k == "od":
            pre.append(f"    let mut {p[1]}_o = [0u8; 8];")
            ae.append(f"Arg::Out {{ dst: &mut {p[1]}_o, len: OutLen::Cap }}")
            post.append(f"    if !{p[1]}.is_null() {{ unsafe {{ *{p[1]} = f64::from_le_bytes({p[1]}_o); }} }}")
        elif k == "oi":
            pre.append(f"    let mut {p[1]}_o = [0u8; {GUEST_SIZE_T}];")
            ae.append(f"Arg::Out {{ dst: &mut {p[1]}_o, len: OutLen::Cap }}")
            post.append(f"    if !{p[1]}.is_null() {{ unsafe {{ *{p[1]} = u32::from_le_bytes({p[1]}_o) as usize; }} }}")
        elif k == "oii":
            pre.append(f"    let mut {p[1]}_o = [0u8; 4];")
            ae.append(f"Arg::Out {{ dst: &mut {p[1]}_o, len: OutLen::Cap }}")
            post.append(f"    if !{p[1]}.is_null() {{ unsafe {{ *{p[1]} = i32::from_le_bytes({p[1]}_o); }} }}")
        elif k == "oev":
            pre.append(f"    let mut {p[1]}_o = [0u8; {elem['bytes']}];")
            ae.append(f"Arg::Out {{ dst: &mut {p[1]}_o, len: OutLen::Cap }}")
            post.append(f"    if !{p[1]}.is_null() {{ unsafe {{ *{p[1]} = {elem['fromle']}::from_le_bytes({p[1]}_o); }} }}")
    L += pre
    args = ", ".join(ae)
    if ret == "e" and elem["callf"] != "call":   # double/float element return
        L.append(f'    let _r = {elem["callf"]}("lind_{name}", &mut [{args}]);'); L += post; L.append("    _r")
    elif ret == "e":                              # int/uint element return
        L.append(f'    let _r = call("lind_{name}", &mut [{args}]);'); L += post; L.append(f'    _r as {elem["retcast"]}')
    elif ret == "d":                              # plain double return (free fns)
        L.append(f'    let _r = call_f64("lind_{name}", &mut [{args}]);'); L += post; L.append("    _r")
    else:                                         # i / sz / v
        L.append(f'    let _r = call("lind_{name}", &mut [{args}]);'); L += post
        if ret == "i":    L.append("    _r as c_int")
        elif ret == "sz": L.append("    _r as usize")
    L.append("}")
    return "#[unsafe(no_mangle)]\n" + "\n".join(L)


def emit_rust(mod):
    out = ["// @generated by gen_gsl_handle.py — do not edit. include!-d by stub/src/lib.rs.",
           "// Uses call/call_f64/call_f32/call_handle/Arg/OutLen from lib.rs; c_int/c_uint/c_void from core::ffi."]
    if mod["class_name"] is not None:   # free-function modules reference other modules' classes
        out.append(f"const {mod['class_name']}: u32 = {mod['class_id']};")
    out.append("")
    for tail, ret, params in mod["funcs"]:
        out.append(rust_fn(mod, tail, ret, params))
        out.append("")
    return "\n".join(out)


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    gen = os.path.join(here, "generated"); os.makedirs(gen, exist_ok=True)
    total = 0
    for mod in MODULES:
        with open(os.path.join(gen, f"{mod['tag']}_shim.inc.c"), "w") as f:
            f.write(emit_c(mod))
        with open(os.path.join(here, "stub", "src", f"{mod['tag']}_generated.rs"), "w") as f:
            f.write(emit_rust(mod))
        cls = f"{mod['class_name']}={mod['class_id']}" if mod.get("class_id") else "free functions"
        print(f"  {mod['tag']}: {len(mod['funcs'])} fns ({cls})")
        total += len(mod["funcs"])
    print(f"generated {total} handle functions across {len(MODULES)} modules")


if __name__ == "__main__":
    main()
