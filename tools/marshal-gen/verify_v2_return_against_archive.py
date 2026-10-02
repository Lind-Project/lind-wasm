#!/usr/bin/env python3
"""Cross-checks every marshal-decision function's CLAIMED argument/return
shape against the REAL compiled function's own wasm-level type in a real
static archive, and COMPLETES or CORRECTS any scalar shape to match --
issue #22's OpenBLAS inference-to-runtime integration, Gate 4: "build the
real OpenBLAS grate".

Why this exists: neither the static-inference tool nor
import_openblas_inference.py's own LLM-lowering path ever has access to
the real compiled library, so both can only assert a function's
argument/return shape from what their own input data proves -- and for
some functions, neither source proves it correctly, or proves it at all.
Investigating the real openblas_lowered.marshal.json found two
independent gaps, both only visible once a REAL archive is in the
picture (which Gate 4 is the first step to introduce):

  - 41 static-inference "proven" records assert `ret: {"kind": "scalar"}`
    with no type/size at all -- fine for V1's ABI-agnostic fpcast-emu
    dispatch, which never needed accurate return metadata, but gen_v2_
    adapter.py's own _v2_scalar_shape_confirmed check refuses to guess
    one for V2. This script FILLS that metadata in from the real archive
    instead of merely detecting the gap: the archive's own ABI is
    authoritative for this exact build, so there is no reason to leave
    these 41 functions on the V1 fallback path once the real answer is
    sitting right there in the .a they're about to link against.
  - 12 LLM-lowered records assert `ret: {"kind": "void"}`
    (import_openblas_inference.py's lower_llm_function hardcodes this for
    every LLM-track function, since the LLM's own prompt manifest never
    carries a return-type field at all) that are NOT actually void --
    cblas_dnrm2/cblas_dsum/cblas_dsdot/cblas_sdsdot and their Fortran-style
    counterparts all compute and return a real scalar (norm/sum/dot
    product). A V2 adapter generated from the wrongly-claimed "void" shape
    discards the real return value in its own generated C and declares a
    mismatched extern prototype, which wasm-ld links against the archive's
    true signature and Binaryen's validator then rejects outright --
    exactly the failure this script exists to catch and CORRECT (not
    merely flag) before compilation.

Neither gap is symbol-specific: this script never special-cases a
function by name. It asks one general, structural question of every
marshal-decision function's every scalar argument/return slot -- "does
its real archive-compiled wasm type agree with what the marshal record
claims (or claims nothing at all)?" -- and, for anything it can express
as a scalar shape, overwrites the claim to match the real archive. A
ptr/handle-kind slot is never rewritten (both schemas already agree it's
a 32-bit address regardless of pointee); a disagreement this script has
no safe scalar shape to express (e.g. a claimed handle/ptr-alias return
whose real wasm type isn't the expected i32, a raw-ABI-slot count that
disagrees with the real archive, or a marshal-decision symbol that isn't
even a defined function in the archive at all) is reported as an
UNRESOLVED finding instead of silently patched or silently skipped,
since there is no general way to repair that shape of mismatch here.
This script itself never fails on an UNRESOLVED finding -- it is a pure
cross-check/completion step, not a policy decision -- so a CALLER that
wants Gate 4's own "fail closed" posture (gate4_manifest.py's own
build_manifest) must check every finding's own `action` for the
"UNRESOLVED" prefix and refuse to proceed if any exist.

Usage:
  verify_v2_return_against_archive.py <lib>.marshal.json <archive>.a \\
      --out-marshal <lib>.verified.marshal.json
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile

_LLVM_BIN_CACHE = None


def _find_llvm_bin():
    global _LLVM_BIN_CACHE
    if _LLVM_BIN_CACHE is not None:
        return _LLVM_BIN_CACHE
    repo_root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    candidates = sorted(
        p for p in (os.path.join(repo_root, d, "bin") for d in os.listdir(repo_root)
                    if d.startswith("clang+llvm-"))
        if os.path.isfile(os.path.join(p, "llvm-nm"))
    )
    if not candidates:
        raise RuntimeError(f"no clang+llvm-*/bin found under {repo_root}")
    _LLVM_BIN_CACHE = candidates[-1]
    return _LLVM_BIN_CACHE


def _symbol_to_member(archive_path):
    """Maps every real defined-function symbol in `archive_path` to the
    .o archive member that defines it, via `llvm-nm -A`."""
    llvm_nm = os.path.join(_find_llvm_bin(), "llvm-nm")
    out = subprocess.run([llvm_nm, "-A", archive_path], capture_output=True, text=True, check=True).stdout
    mapping = {}
    for line in out.splitlines():
        m = re.match(r"^.*:(\S+\.o):\s+[0-9a-fA-F]+\s+T\s+(\S+)$", line)
        if m:
            member, sym = m.groups()
            mapping[sym] = member
    return mapping


def real_function_wasm_type(archive_path, symbol, sym_to_member, workdir):
    """Returns (param_types, ret_type) for `symbol`'s real archive-
    compiled wasm function type: `param_types` is a list of wasm
    valtypes ("i32"/"i64"/"f32"/"f64"), one per raw ABI slot in order;
    `ret_type` is "nil" for void or a single valtype. None if `symbol`
    isn't a defined function in this archive at all."""
    member = sym_to_member.get(symbol)
    if member is None:
        return None
    llvm_ar = os.path.join(_find_llvm_bin(), "llvm-ar")
    member_path = os.path.join(workdir, member)
    if not os.path.exists(member_path):
        with open(member_path, "wb") as fh:
            fh.write(subprocess.run([llvm_ar, "p", archive_path, member],
                                     capture_output=True, check=True).stdout)
    dump = subprocess.run(["wasm-objdump", "-x", member_path], capture_output=True, text=True, check=True).stdout
    fm = re.search(rf"func\[(\d+)\] sig=(\d+) <{re.escape(symbol)}>", dump)
    if not fm:
        return None
    sig = fm.group(2)
    tm = re.search(rf"type\[{sig}\] \(([^)]*)\) -> (\S+)", dump)
    if not tm:
        return None
    params_str, ret_type = tm.group(1), tm.group(2)
    params = [p.strip() for p in params_str.split(",")] if params_str.strip() else []
    return params, ret_type


# A confirmed scalar shape's own (type, size) <-> the wasm valtype
# gen_v2_adapter.py's wasm_scalar_ctype would map it to -- kept as one
# bidirectional table so completing a shape FROM a real wasm valtype and
# checking a claimed shape AGAINST one can never drift apart.
_SCALAR_SHAPE_TO_WASM_TYPE = {("double", 8): "f64", ("float", 4): "f32",
                              ("int", 8): "i64", ("int", 4): "i32"}
_WASM_TYPE_TO_SCALAR_SHAPE = {v: {"kind": "scalar", "type": k[0], "size": k[1]}
                              for k, v in _SCALAR_SHAPE_TO_WASM_TYPE.items()}


def _claimed_scalar_wasm_type(node):
    """The wasm valtype a CONFIRMED scalar `node` claims, or None if its
    type/size isn't one of the shapes gen_v2_adapter.py's own
    _v2_scalar_shape_confirmed recognizes (including a bare
    `{"kind": "scalar"}` with no type/size at all)."""
    return _SCALAR_SHAPE_TO_WASM_TYPE.get((node.get("type"), node.get("size")))


def _complete_or_correct_arg(arg, real_type, symbol, index, findings):
    """In place: if `arg` is scalar-kind and `real_type` is a recognized
    scalar valtype that disagrees with (or completes) what `arg` claims,
    overwrites `arg`'s own type/size to match -- the real archive's ABI
    is authoritative for this exact build. A ptr/handle-kind arg is
    checked (its real type must be "i32", since both always resolve to a
    32-bit address regardless of pointee) but never rewritten; a
    disagreement there is appended to `findings` as UNRESOLVED, since
    there is no scalar shape to express a ptr/handle mismatch as."""
    kind = arg.get("kind", "scalar")
    if kind in ("ptr", "handle"):
        if real_type != "i32":
            findings.append({"symbol": symbol, "location": f"arg{index}",
                              "claimed": "i32", "real": real_type,
                              "action": f"UNRESOLVED: {kind} argument's real wasm type disagrees"})
        return
    claimed = _claimed_scalar_wasm_type(arg)
    if real_type not in _WASM_TYPE_TO_SCALAR_SHAPE:
        if claimed is not None:
            findings.append({"symbol": symbol, "location": f"arg{index}",
                              "claimed": claimed, "real": real_type,
                              "action": "UNRESOLVED: real wasm type is not a recognized scalar valtype"})
        return
    if claimed == real_type:
        return
    action = "completed unconfirmed scalar argument" if claimed is None else \
        "corrected disagreeing scalar argument"
    findings.append({"symbol": symbol, "location": f"arg{index}", "claimed": claimed,
                      "real": real_type, "action": action})
    new_shape = _WASM_TYPE_TO_SCALAR_SHAPE[real_type]
    arg["type"], arg["size"] = new_shape["type"], new_shape["size"]


def _complete_or_correct_return(f, real_type, findings):
    """Same reasoning as _complete_or_correct_arg, for the return
    position's own wider kind vocabulary (void/scalar/handle/
    ptr_alias_arg/ptr_into_arg): a void return whose real type isn't
    "nil", or an unconfirmed/disagreeing scalar return, is completed or
    corrected to the real archive's own scalar shape; a handle/
    ptr_alias_arg/ptr_into_arg return (always a 32-bit address) is
    checked against "i32" but never rewritten -- there is no scalar
    shape to express that kind of disagreement as."""
    symbol = f["name"]
    ret = f.get("ret") or {}
    kind = ret.get("kind", "void")
    if kind in ("handle", "ptr_alias_arg", "ptr_into_arg"):
        if real_type != "i32":
            findings.append({"symbol": symbol, "location": "ret", "claimed": "i32", "real": real_type,
                              "action": f"UNRESOLVED: {kind} return's real wasm type disagrees"})
        return
    if kind == "void":
        if real_type == "nil":
            return
        if real_type not in _WASM_TYPE_TO_SCALAR_SHAPE:
            findings.append({"symbol": symbol, "location": "ret", "claimed": "nil", "real": real_type,
                              "action": "UNRESOLVED: real wasm type is not a recognized scalar valtype"})
            return
        findings.append({"symbol": symbol, "location": "ret", "claimed": "nil", "real": real_type,
                          "action": "completed void -> real scalar return"})
        f["ret"] = _WASM_TYPE_TO_SCALAR_SHAPE[real_type]
        return
    if kind != "scalar":
        # An unsupported return kind (e.g. "ptr_to_static") that neither
        # gen_grate.py's nor gen_v2_adapter.py's own SUPPORTED_RET
        # recognizes at all -- a completely different, pre-existing
        # rejection this script has no business touching. Falling
        # through to the scalar-completion logic below would wrongly
        # treat it as an unconfirmed scalar and "complete" it into one.
        return
    claimed = _claimed_scalar_wasm_type(ret)
    if real_type not in _WASM_TYPE_TO_SCALAR_SHAPE:
        if claimed is not None:
            findings.append({"symbol": symbol, "location": "ret", "claimed": claimed, "real": real_type,
                              "action": "UNRESOLVED: real wasm type is not a recognized scalar valtype"})
        return
    if claimed == real_type:
        return
    action = "completed unconfirmed scalar return" if claimed is None else \
        "corrected disagreeing scalar return"
    findings.append({"symbol": symbol, "location": "ret", "claimed": claimed, "real": real_type,
                      "action": action})
    f["ret"] = _WASM_TYPE_TO_SCALAR_SHAPE[real_type]


def verify_and_patch(marshal_path, archive_path):
    """Returns (functions, findings): `functions` is the (possibly
    completed/corrected) function list, `findings` is a list of
    {symbol, location, claimed, real, action} dicts, one per
    argument/return slot this script changed OR flagged as UNRESOLVED
    (an "action" starting with "UNRESOLVED" was NOT patched -- callers
    that want to fail closed on those should check for the prefix)."""
    with open(marshal_path) as fh:
        marshal = json.load(fh)
    fns = marshal["functions"]
    sym_to_member = _symbol_to_member(archive_path)

    findings = []
    with tempfile.TemporaryDirectory() as workdir:
        for f in fns:
            if f.get("decision") != "marshal":
                continue
            real = real_function_wasm_type(archive_path, f["name"], sym_to_member, workdir)
            if real is None:
                # A marshal-decision symbol this grate is about to
                # register a handler for does not even exist as a
                # defined function in the archive it's about to link
                # against -- never a harmless gap to skip past. Left
                # unresolved, generation would either fail to link at
                # all or (worse, if some other symbol of the same name
                # exists elsewhere) silently bind to the wrong function.
                findings.append({"symbol": f["name"], "location": "function", "claimed": None,
                                  "real": None,
                                  "action": "UNRESOLVED: symbol not found as a defined function in the real archive"})
                continue
            real_params, real_ret = real
            args = f.get("args", [])
            if len(args) != len(real_params):
                findings.append({"symbol": f["name"], "location": "args", "claimed": len(args),
                                  "real": len(real_params),
                                  "action": "UNRESOLVED: raw ABI slot count disagrees with the real archive"})
            else:
                for i, (arg, real_type) in enumerate(zip(args, real_params)):
                    _complete_or_correct_arg(arg, real_type, f["name"], i, findings)
            _complete_or_correct_return(f, real_ret, findings)
    return fns, findings


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("marshal_json", help="<lib>.marshal.json (gen_grate-shaped, from import_openblas_inference.py)")
    ap.add_argument("archive", help="the real static archive (.a) this grate will link against")
    ap.add_argument("--out-marshal", required=True, help="output, verified/patched <lib>.marshal.json")
    ap.add_argument("--out-findings", help="optional output JSON file for the full findings list")
    args = ap.parse_args()

    with open(args.marshal_json) as fh:
        marshal = json.load(fh)
    fns, findings = verify_and_patch(args.marshal_json, args.archive)
    marshal["functions"] = fns
    with open(args.out_marshal, "w") as fh:
        json.dump(marshal, fh, indent=2)
        fh.write("\n")
    if args.out_findings:
        with open(args.out_findings, "w") as fh:
            json.dump(findings, fh, indent=2)
            fh.write("\n")

    unresolved = [f for f in findings if f["action"].startswith("UNRESOLVED")]
    print(f"[verify_v2_return_against_archive] {len(findings)} finding(s) "
          f"({len(findings) - len(unresolved)} completed/corrected, {len(unresolved)} unresolved)")
    for finding in findings:
        print(f"  - {finding['symbol']} {finding['location']}: claimed {finding['claimed']!r}, "
              f"real archive says {finding['real']!r} -- {finding['action']}", file=sys.stderr)
    print(f"[verify_v2_return_against_archive] wrote {args.out_marshal}")


if __name__ == "__main__":
    main()
