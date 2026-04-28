//! Remote library call server — scalar and pointer argument support.
//!
//! Loads a native shared library, maps call_id → function pointer, listens on
//! a Unix domain socket, and executes library functions on behalf of a local
//! Lind cage.
//!
//! Usage:
//!   lind-remote-server <server_config.json>
//!
//! Server config format:
//! {
//!   "library":  "/path/to/libfoo.so",
//!   "endpoint": "unix:///tmp/foo.sock",
//!   "functions": [
//!     { "call_id": 1, "symbol": "fill", "num_args": 2, "ret": "i32",
//!       "args": [
//!         { "type": "ptr", "direction": "out", "size_arg": 1 },
//!         { "type": "scalar" }
//!       ]
//!     }
//!   ]
//! }
//!
//! Pointer directions: "in" (client→server), "out" (server→client),
//! "inout" (both).  `size_arg` is the 0-based index of the scalar argument
//! that carries the buffer size.  Omit `args` for scalar-only functions.

use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::os::unix::net::{UnixListener, UnixStream};

use anyhow::{anyhow, Result};
use lind_remote_lib::{
    read_call_id, read_ptr_sections, read_scalar_args, write_response, write_response_with_ptrs,
    Direction,
};
use serde::Deserialize;

// ---- Config ----

#[derive(Deserialize)]
struct ServerConfig {
    library: String,
    endpoint: String,
    functions: Vec<FunctionEntry>,
}

#[derive(Deserialize)]
struct ArgSpecConfig {
    #[serde(rename = "type", default = "default_arg_type")]
    ty: String,
    direction: Option<String>,
    size_arg: Option<usize>,
}

fn default_arg_type() -> String {
    "scalar".to_string()
}

#[derive(Deserialize)]
struct FunctionEntry {
    call_id: u32,
    symbol: String,
    num_args: usize,
    ret: String,
    #[serde(default)]
    args: Vec<ArgSpecConfig>,
}

// ---- Function registry ----

enum ParsedArgSpec {
    Scalar,
    Ptr { direction: Direction, size_arg: usize },
}

struct LoadedFn {
    ptr: *mut libc::c_void,
    #[allow(dead_code)]
    num_args: usize,
    ret: String,
    arg_specs: Vec<ParsedArgSpec>,
}

// SAFETY: function pointers remain valid for the lifetime of the loaded library handle,
// which lives for the entire process lifetime of this server.
unsafe impl Send for LoadedFn {}
unsafe impl Sync for LoadedFn {}

// ---- Native scalar call trampoline ----
//
// All arguments are passed as i64. On the x86-64 System V ABI integer
// arguments go into general-purpose registers (rdi, rsi, rdx, rcx, r8, r9).
// Pointer arguments fit in 64-bit registers without truncation.
// Maximum of 6 arguments is supported.

unsafe fn call_scalar_native(func_ptr: *mut libc::c_void, args: &[u64], ret: &str) -> u64 {
    let a = |i: usize| args.get(i).copied().unwrap_or(0) as i64;

    let raw: i64 = match args.len() {
        0 => {
            let f: unsafe extern "C" fn() -> i64 = std::mem::transmute(func_ptr);
            f()
        }
        1 => {
            let f: unsafe extern "C" fn(i64) -> i64 = std::mem::transmute(func_ptr);
            f(a(0))
        }
        2 => {
            let f: unsafe extern "C" fn(i64, i64) -> i64 = std::mem::transmute(func_ptr);
            f(a(0), a(1))
        }
        3 => {
            let f: unsafe extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(func_ptr);
            f(a(0), a(1), a(2))
        }
        4 => {
            let f: unsafe extern "C" fn(i64, i64, i64, i64) -> i64 =
                std::mem::transmute(func_ptr);
            f(a(0), a(1), a(2), a(3))
        }
        5 => {
            let f: unsafe extern "C" fn(i64, i64, i64, i64, i64) -> i64 =
                std::mem::transmute(func_ptr);
            f(a(0), a(1), a(2), a(3), a(4))
        }
        6 => {
            let f: unsafe extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64 =
                std::mem::transmute(func_ptr);
            f(a(0), a(1), a(2), a(3), a(4), a(5))
        }
        n => {
            eprintln!("remote-server: too many args ({n}); max is 6");
            0
        }
    };

    if ret == "void" { 0 } else { raw as u64 }
}

// ---- Request handlers ----

fn handle_scalar_request(
    stream: &mut UnixStream,
    _call_id: u32,
    entry: &LoadedFn,
) -> Result<()> {
    let args = read_scalar_args(stream)?;
    let result = unsafe { call_scalar_native(entry.ptr, &args, &entry.ret) };
    let errno_val = unsafe { *libc::__errno_location() } as i32;
    write_response(stream, result, errno_val)
}

fn handle_ptr_request(
    stream: &mut UnixStream,
    _call_id: u32,
    entry: &LoadedFn,
) -> Result<()> {
    // Collect directions of the Ptr args in declaration order for the wire reader.
    let ptr_directions: Vec<Direction> = entry
        .arg_specs
        .iter()
        .filter_map(|s| {
            if let ParsedArgSpec::Ptr { direction, .. } = s {
                Some(direction.clone())
            } else {
                None
            }
        })
        .collect();

    let mut args = read_scalar_args(stream)?;
    let ptr_bufs = read_ptr_sections(stream, &ptr_directions)?;

    // Allocate native buffers and patch args with native pointers.
    // native_bufs keeps the allocations alive across the native call.
    let mut native_bufs: Vec<Vec<u8>> = Vec::new();
    let mut ptr_buf_idx = 0;
    for (i, spec) in entry.arg_specs.iter().enumerate() {
        if let ParsedArgSpec::Ptr { .. } = spec {
            let received = &ptr_bufs[ptr_buf_idx];
            let mut buf = vec![0u8; received.alloc_size as usize];
            if received.direction != Direction::Out {
                let copy_len = received.data.len().min(buf.len());
                buf[..copy_len].copy_from_slice(&received.data[..copy_len]);
            }
            if i < args.len() {
                args[i] = buf.as_ptr() as u64;
            }
            native_bufs.push(buf);
            ptr_buf_idx += 1;
        }
    }

    let result = unsafe { call_scalar_native(entry.ptr, &args, &entry.ret) };
    let errno_val = unsafe { *libc::__errno_location() } as i32;

    // Collect output buffers for Out/InOut args.
    let mut out_bufs: Vec<Vec<u8>> = Vec::new();
    let mut native_idx = 0;
    for spec in &entry.arg_specs {
        if let ParsedArgSpec::Ptr { direction, .. } = spec {
            if *direction == Direction::Out || *direction == Direction::InOut {
                out_bufs.push(native_bufs[native_idx].clone());
            }
            native_idx += 1;
        }
    }

    write_response_with_ptrs(stream, result, errno_val, &out_bufs)
}

fn handle_request(stream: &mut UnixStream, registry: &HashMap<u32, LoadedFn>) -> Result<()> {
    let call_id = read_call_id(stream)?;

    let entry = registry
        .get(&call_id)
        .ok_or_else(|| anyhow!("unknown call_id {call_id}"))?;

    let has_ptrs = entry
        .arg_specs
        .iter()
        .any(|s| matches!(s, ParsedArgSpec::Ptr { .. }));

    if has_ptrs {
        handle_ptr_request(stream, call_id, entry)
    } else {
        handle_scalar_request(stream, call_id, entry)
    }
}

// ---- Server loop ----

fn run(config_path: &str) -> Result<()> {
    let content = fs::read_to_string(config_path)?;
    let config: ServerConfig = serde_json::from_str(&content)?;

    let socket_path = config
        .endpoint
        .strip_prefix("unix://")
        .ok_or_else(|| anyhow!("invalid endpoint: {}", config.endpoint))?;

    // Load the shared library.
    let lib_cstr = CString::new(config.library.as_str())?;
    let lib_handle =
        unsafe { libc::dlopen(lib_cstr.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if lib_handle.is_null() {
        let err_ptr = unsafe { libc::dlerror() };
        let msg = if err_ptr.is_null() {
            "unknown error".to_string()
        } else {
            unsafe { std::ffi::CStr::from_ptr(err_ptr) }
                .to_string_lossy()
                .into_owned()
        };
        return Err(anyhow!("dlopen failed: {msg}"));
    }

    // Resolve symbols and build the call_id registry.
    let mut registry: HashMap<u32, LoadedFn> = HashMap::new();
    for entry in &config.functions {
        let sym_cstr = CString::new(entry.symbol.as_str())?;
        let ptr = unsafe { libc::dlsym(lib_handle, sym_cstr.as_ptr()) };
        if ptr.is_null() {
            eprintln!("remote-server: symbol not found: {}", entry.symbol);
            continue;
        }

        let arg_specs: Vec<ParsedArgSpec> = entry
            .args
            .iter()
            .map(|a| {
                if a.ty == "ptr" {
                    let direction = match a.direction.as_deref() {
                        Some("in") => Direction::In,
                        Some("out") => Direction::Out,
                        Some("inout") => Direction::InOut,
                        _ => Direction::In,
                    };
                    ParsedArgSpec::Ptr {
                        direction,
                        size_arg: a.size_arg.unwrap_or(0),
                    }
                } else {
                    ParsedArgSpec::Scalar
                }
            })
            .collect();

        registry.insert(
            entry.call_id,
            LoadedFn {
                ptr,
                num_args: entry.num_args,
                ret: entry.ret.clone(),
                arg_specs,
            },
        );
        println!(
            "remote-server: registered {} (call_id={})",
            entry.symbol, entry.call_id
        );
    }

    // Bind the Unix socket.
    let _ = fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    println!("remote-server: listening on {socket_path}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut s) => {
                if let Err(e) = handle_request(&mut s, &registry) {
                    eprintln!("remote-server: request error: {e}");
                }
            }
            Err(e) => eprintln!("remote-server: accept error: {e}"),
        }
    }

    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: lind-remote-server <server_config.json>");
        std::process::exit(1);
    }
    if let Err(e) = run(&args[1]) {
        eprintln!("remote-server: fatal: {e}");
        std::process::exit(1);
    }
}
