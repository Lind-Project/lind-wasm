#!/usr/bin/env bash
# Generate the extern "C" stub layer (stub/src/lib.rs) for a sandboxed-lib example
# from a functions.txt manifest.
#
#   Usage: gen_stubs.sh <functions.txt> > stub/src/lib.rs
#
# functions.txt format (one per line; '#' comments and blank lines ignored):
#
#   <name> <ret-type> <arg-spec>...
#
# Return types:  i32 | usize | void
#
# Argument specs:
#   i32                     scalar int, passed by value
#   usize                   scalar size_t, passed by value
#   cstr                    const char* input — copied INTO the guest (marshalling)
#   outbuf,cap=C,len=X      caller-allocated char* output buffer:
#                             cap=C  capacity is the value of arg #C (1-based)
#                             len=X  how many bytes to copy back out — one of:
#                                      ret    = the function's return value
#                                      nul    = up to & including the first NUL
#                                      cap    = the whole buffer (capacity)
#                                      arg<M> = the value the guest wrote into outlen arg #M
#   inoutbuf,cap=C,len=X    same, but the buffer's existing contents are also copied
#                             INTO the guest first — for in-place transforms, where
#                             the guest reads the buffer as well as writing it
#   outlen                  size_t* output param — the guest writes a length there,
#                             read back into the caller's size_t and usable as len=arg<M>
#   struct=NAME             const struct NAME* input — marshalled field by field (the
#                             host LP64 and guest ILP32 layouts differ). NAME must be
#                             declared by a struct line (see below).
#
# Struct declarations (anywhere in the file, before or after the functions):
#
#   struct NAME { <ftype> <field>; <ftype> <field>; ... }
#
# where <ftype> is one of:  i32 (int/float) | long | i64 (int64/double) | cstr (char*)
#
# A function whose args + return are all `i32` gets a trivial pass-through stub.
# Anything else is a *marshalled* stub built from an `Arg` array. This is the seam
# where richer types get added later.
set -euo pipefail

funcs="${1:?usage: gen_stubs.sh <functions.txt>}"

# Echo "ftype:field ftype:field ..." for the struct named $1, read from its
# `struct NAME { ... }` declaration in the manifest.
struct_fields() {
    local want="$1" line body f ftype fname out
    while IFS= read -r line; do
        line="${line%%#*}"
        # shellcheck disable=SC2086
        set -- $line
        [ "${1:-}" = "struct" ] || continue
        [ "${2:-}" = "$want" ] || continue
        body="${line#*\{}"; body="${body%\}*}"
        out=""
        local IFS=';'
        for f in $body; do
            f="${f#"${f%%[![:space:]]*}"}"   # ltrim
            f="${f%"${f##*[![:space:]]}"}"   # rtrim
            [ -z "$f" ] && continue
            ftype="${f%% *}"; fname="${f##* }"
            out="$out ${ftype}:${fname}"
        done
        printf '%s\n' "${out# }"
        return 0
    done < "$funcs"
    echo "gen_stubs.sh: unknown struct '$want'" >&2
    return 1
}

# Emit a #[repr(C)] Rust struct for every `struct NAME { ... }` in the manifest, so
# the stub can read the caller's struct with the host's native field layout.
emit_structs() {
    local line sname fd ft fn
    while IFS= read -r line; do
        line="${line%%#*}"
        # shellcheck disable=SC2086
        set -- $line
        [ "${1:-}" = "struct" ] || continue
        sname="$2"
        printf '\n#[repr(C)]\n#[allow(dead_code)]\nstruct %s {\n' "$sname"
        for fd in $(struct_fields "$sname"); do
            ft="${fd%%:*}"; fn="${fd#*:}"
            case "$ft" in
                i32)  printf '    %s: c_int,\n' "$fn" ;;
                long) printf '    %s: c_long,\n' "$fn" ;;
                i64)  printf '    %s: c_longlong,\n' "$fn" ;;
                cstr) printf '    %s: *const c_char,\n' "$fn" ;;
                *) echo "gen_stubs.sh: struct $sname: unknown field type '$ft'" >&2; exit 2 ;;
            esac
        done
        printf '}\n'
    done < "$funcs"
}

# --- pass 1: classify what helpers/imports we need -------------------------------
has_scalar=0      # an all-i32 function -> emit the scalar `call` helper
has_marshal=0     # any marshalled function -> emit Arg-array helpers + imports
has_cstr=0        # any cstr (arg or struct field) -> need CStr + cstr_bytes
has_ptr=0         # any cstr/outbuf/outlen arg -> need c_char / pointer params
has_struct=0      # any struct= arg -> need Field + emit repr(C) structs
need_c_int=0      # struct i32 field  -> need core::ffi::c_int
need_c_long=0     # struct long field -> need core::ffi::c_long
need_c_longlong=0 # struct i64 field  -> need core::ffi::c_longlong
while IFS= read -r line; do
    line="${line%%#*}"
    [ -z "${line// /}" ] && continue
    # shellcheck disable=SC2086
    set -- $line
    [ "$1" = "struct" ] && continue    # struct declaration, not a function
    _name="$1"; shift
    ret="$1"; shift
    marshal=0
    [ "$ret" != "i32" ] && marshal=1
    for t in "$@"; do
        case "$t" in
            i32) ;;
            usize) marshal=1 ;;
            cstr) marshal=1; has_cstr=1; has_ptr=1 ;;
            outbuf,*|inoutbuf,*) marshal=1; has_ptr=1 ;;
            outlen) marshal=1; has_ptr=1 ;;
            struct=*)
                marshal=1; has_struct=1
                for fd in $(struct_fields "${t#struct=}"); do
                    case "${fd%%:*}" in
                        i32)  need_c_int=1 ;;
                        long) need_c_long=1 ;;
                        i64)  need_c_longlong=1 ;;
                        cstr) has_cstr=1; has_ptr=1 ;;
                    esac
                done
                ;;
            *) echo "gen_stubs.sh: unknown arg spec '$t'" >&2; exit 2 ;;
        esac
    done
    if [ "$marshal" -eq 1 ]; then has_marshal=1; else has_scalar=1; fi
done < "$funcs"

# --- header: banner ---------------------------------------------------------------
cat <<'HEADER'
// @generated by tools/gen_stubs.sh from functions.txt — do not edit by hand.
// Regenerate with `make gen`.
//
// Native shared library whose exported symbols run their real implementations
// inside the lind/wasmtime sandbox. A native app links this like any other .so
// and calls the symbols with no knowledge that the work happens in a wasm guest.
// On the first call the lind runtime is brought up and the guest module is
// instantiated as a long-lived sandboxed library; later calls reuse it.

use std::sync::{Mutex, OnceLock};
HEADER

# --- header: imports (marshalling pulls in a couple more) -------------------------
# core::ffi: collect exactly the C types the stubs reference, in a stable order.
ffi=""
add_ffi() { case " $ffi " in *" $1 "*) ;; *) ffi="${ffi:+$ffi }$1" ;; esac; }
[ "$has_cstr" -eq 1 ] && add_ffi CStr
{ [ "$has_cstr" -eq 1 ] || [ "$has_ptr" -eq 1 ]; } && add_ffi c_char
[ "$need_c_int" -eq 1 ] && add_ffi c_int
[ "$need_c_long" -eq 1 ] && add_ffi c_long
[ "$need_c_longlong" -eq 1 ] && add_ffi c_longlong
if [ -n "$ffi" ]; then
    printf '\nuse core::ffi::{%s};\n' "$(echo "$ffi" | tr ' ' ',' | sed 's/,/, /g')"
fi
if [ "$has_marshal" -eq 1 ]; then
    if [ "$has_struct" -eq 1 ]; then
        printf '\nuse lind_boot::{Arg, CliOptions, Field, OutLen, SandboxedLib, init_sandboxed_lib};\n'
    else
        printf '\nuse lind_boot::{Arg, CliOptions, OutLen, SandboxedLib, init_sandboxed_lib};\n'
    fi
else
    printf '\nuse lind_boot::{CliOptions, SandboxedLib, init_sandboxed_lib};\n'
fi

# repr(C) mirrors of the manifest's struct declarations (so the stub reads the
# caller's struct with the host's native layout).
if [ "$has_struct" -eq 1 ]; then
    emit_structs
fi

# --- header: lazy-init rationale + the resident instance --------------------------
cat <<'CORE'

// Why the guest is initialized lazily (on the first call) rather than in a
// load-time constructor (C `__attribute__((constructor))` / Rust `#[ctor]`, which
// place a function in `.init_array` for the loader to run at dlopen/startup).
// `init_sandboxed_lib` is heavyweight — it brings up the whole lind runtime
// (RawPOSIX, the 3i trampoline + syscall handlers, the wasmtime engine, grate
// worker threads, a SIGUSR2 handler) and instantiates the guest. A constructor is
// the wrong place for that:
//
//   1. No error handling. A constructor returns void and runs before the app's
//      `main`. If init fails, the only option is to abort the whole process before
//      it starts — you cannot return an error the app can handle. Lazy init lets
//      the first call report failure normally.
//   2. Threads at load time. Init spawns threads (grate workers). Creating threads
//      from a constructor is hazardous: e.g. if the app later `fork()`s, only the
//      calling thread survives in the child, leaving the others' locks/state
//      broken. Many runtimes forbid pre-`main` thread creation.
//   3. Signal handlers at load time. Init installs a SIGUSR2 handler; doing that
//      during load can clobber the host app's signal setup before it is ready.
//   4. Ordering / dlopen lock. Constructors run while the dynamic loader holds its
//      internal lock and while other libraries' constructors and the language
//      runtime may not be fully up. Heavy init here (tokio, RawPOSIX, anything that
//      resolves symbols) risks deadlock and static-init-order fragility.
//   5. Pay only when used. If the app loads the .so but never calls a function, a
//      constructor still does all the expensive work; lazy init costs nothing until
//      first use.
//
// So `OnceLock` defers the heavy, thread-spawning, signal-installing work until the
// app actually calls in — by which point the process is fully initialized, we're on
// a normal thread, and errors can be handled. It runs the init exactly once (a cheap
// atomic load on every call thereafter). The `Mutex` serializes calls because a
// wasmtime `Store` is `Send` but not `Sync`.
static LIB: OnceLock<Mutex<SandboxedLib>> = OnceLock::new();

/// Path to the precompiled guest module. Overridable via `LIND_MODULE`; defaults
/// to `guest.cwasm` in the process's working directory.
fn module_path() -> String {
    std::env::var("LIND_MODULE").unwrap_or_else(|_| "guest.cwasm".to_string())
}

fn lib() -> &'static Mutex<SandboxedLib> {
    LIB.get_or_init(|| {
        let cli = CliOptions::for_sandboxed_lib(module_path());
        let sandboxed_lib = init_sandboxed_lib(cli)
            .unwrap_or_else(|e| panic!("lind sandboxed-lib init failed: {e:?}"));
        Mutex::new(sandboxed_lib)
    })
}
CORE

# --- header: the scalar forwarding helper (only if needed) ------------------------
if [ "$has_scalar" -eq 1 ]; then
    cat <<'SCALAR'

fn call(name: &str, args: &[i32]) -> i32 {
    lib()
        .lock()
        .unwrap()
        .call_scalar(name, args)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
}
SCALAR
fi

# --- header: the marshalling helpers (only if needed) -----------------------------
if [ "$has_cstr" -eq 1 ]; then
    cat <<'CSTR'

/// Read a host C string into an owned buffer INCLUDING the trailing NUL, so that
/// after it is copied into guest memory the guest still sees a valid C string.
unsafe fn cstr_bytes(p: *const c_char) -> Vec<u8> {
    if p.is_null() {
        return vec![0];
    }
    unsafe { CStr::from_ptr(p) }.to_bytes_with_nul().to_vec()
}
CSTR
fi
if [ "$has_marshal" -eq 1 ]; then
    cat <<'MARSHAL'

fn call_buf(name: &str, args: &mut [Arg]) -> i64 {
    lib()
        .lock()
        .unwrap()
        .call(name, args)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
}
MARSHAL
fi

# --- per-function stubs -----------------------------------------------------------
while IFS= read -r line; do
    line="${line%%#*}"
    [ -z "${line// /}" ] && continue
    # shellcheck disable=SC2086
    set -- $line
    [ "$1" = "struct" ] && continue    # struct declaration, handled by emit_structs
    name="$1"; shift
    ret="$1"; shift

    # scalar fast path: all-i32 args + i32 return.
    scalar=1
    [ "$ret" != "i32" ] && scalar=0
    for t in "$@"; do [ "$t" != "i32" ] && scalar=0; done

    if [ "$scalar" -eq 1 ]; then
        params=""; slice=""; i=0
        for _ in "$@"; do
            [ "$i" -gt 0 ] && { params="$params, "; slice="$slice, "; }
            params="${params}a${i}: i32"
            slice="${slice}a${i}"
            i=$((i + 1))
        done
        printf '\n#[unsafe(no_mangle)]\npub extern "C" fn %s(%s) -> i32 {\n    call("%s", &[%s])\n}\n' \
            "$name" "$params" "$name" "$slice"
        continue
    fi

    # marshalled path: build params, a copy-in/alloc preamble, and the Arg array.
    params=""; preamble=""; argexpr=""; i=0
    for t in "$@"; do
        [ "$i" -gt 0 ] && { params="$params, "; argexpr="$argexpr,"; }
        case "$t" in
            i32)
                params="${params}a${i}: i32"
                argexpr="${argexpr}\n        Arg::I32(a${i})"
                ;;
            usize)
                params="${params}a${i}: usize"
                argexpr="${argexpr}\n        Arg::USize(a${i})"
                ;;
            cstr)
                params="${params}a${i}: *const c_char"
                preamble="${preamble}    let in${i} = unsafe { cstr_bytes(a${i}) };\n"
                argexpr="${argexpr}\n        Arg::Buf(&in${i})"
                ;;
            outbuf,*|inoutbuf,*)
                # `Out` vs `InOut` differ only in whether the caller's current bytes
                # are copied into the guest before the call; the cap=/len= parsing and
                # the copy-back are identical.
                case "$t" in
                    inoutbuf,*) variant="InOut"; bufvar="io" ;;
                    *)          variant="Out";   bufvar="out" ;;
                esac
                # parse cap=C and len=X out of the comma-separated spec
                cap=""; lenspec=""
                IFS=, read -ra fields <<< "$t"
                for f in "${fields[@]}"; do
                    case "$f" in
                        cap=*) cap="${f#cap=}" ;;
                        len=*) lenspec="${f#len=}" ;;
                    esac
                done
                [ -z "$cap" ] && { echo "gen_stubs.sh: ${t%%,*} missing cap= in '$t' ($name)" >&2; exit 2; }
                capidx=$((cap - 1))   # 1-based arg # -> 0-based param name
                case "$lenspec" in
                    ret) outlen="OutLen::Ret" ;;
                    nul) outlen="OutLen::Nul" ;;
                    cap) outlen="OutLen::Cap" ;;
                    arg*) outlen="OutLen::FromArg($(( ${lenspec#arg} - 1 )))" ;;
                    *) echo "gen_stubs.sh: ${t%%,*} bad len='$lenspec' in '$t' ($name)" >&2; exit 2 ;;
                esac
                params="${params}a${i}: *mut c_char"
                preamble="${preamble}    let ${bufvar}${i} = unsafe { core::slice::from_raw_parts_mut(a${i} as *mut u8, a${capidx} as usize) };\n"
                argexpr="${argexpr}\n        Arg::${variant} { dst: ${bufvar}${i}, len: ${outlen} }"
                ;;
            outlen)
                params="${params}a${i}: *mut usize"
                preamble="${preamble}    let len${i} = unsafe { &mut *a${i} };\n"
                argexpr="${argexpr}\n        Arg::OutLen(len${i})"
                ;;
            struct=*)
                # const struct NAME* input: borrow the caller's struct (host layout via
                # repr(C)), copy in any pointer fields, and build a Field array in
                # declaration order for the engine to pack into the guest layout.
                sname="${t#struct=}"
                params="${params}a${i}: *const ${sname}"
                preamble="${preamble}    let s${i} = unsafe { &*a${i} };\n"
                farr=""; j=0
                for fd in $(struct_fields "$sname"); do
                    [ "$j" -gt 0 ] && farr="${farr},"
                    ft="${fd%%:*}"; fn="${fd#*:}"
                    case "$ft" in
                        i32)  farr="${farr}\n        Field::I32(s${i}.${fn})" ;;
                        long) farr="${farr}\n        Field::Long(s${i}.${fn} as i64)" ;;
                        i64)  farr="${farr}\n        Field::I64(s${i}.${fn} as i64)" ;;
                        cstr)
                            preamble="${preamble}    let s${i}_${fn} = unsafe { cstr_bytes(s${i}.${fn}) };\n"
                            farr="${farr}\n        Field::Ptr(&s${i}_${fn})"
                            ;;
                    esac
                    j=$((j + 1))
                done
                preamble="${preamble}    let f${i} = [${farr}\n    ];\n"
                argexpr="${argexpr}\n        Arg::StructIn(&f${i})"
                ;;
        esac
        i=$((i + 1))
    done

    case "$ret" in
        i32)   sig_ret=" -> i32";   ret_expr="    call_buf(\"$name\", &mut args) as i32\n" ;;
        usize) sig_ret=" -> usize"; ret_expr="    call_buf(\"$name\", &mut args) as usize\n" ;;
        void)  sig_ret="";          ret_expr="    call_buf(\"$name\", &mut args);\n" ;;
        *)     echo "gen_stubs.sh: unknown return type '$ret' for '$name'" >&2; exit 2 ;;
    esac

    printf '\n#[unsafe(no_mangle)]\npub extern "C" fn %s(%s)%s {\n' "$name" "$params" "$sig_ret"
    [ -n "$preamble" ] && printf '%b' "$preamble"
    printf '    let mut args = [%b\n    ];\n' "$argexpr"
    printf '%b}\n' "$ret_expr"
done < "$funcs"
