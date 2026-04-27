//! Remote library call server — scalar-only implementation.
//!
//! Loads a native shared library, maps call_id → function pointer, listens on
//! a Unix domain socket, and executes scalar (integer) library functions on
//! behalf of a local Lind cage.
//!
//! Usage:
//!   lind-remote-server <server_config.json>
//!
//! Server config format:
//! {
//!   "library":  "/path/to/libfoo.so",
//!   "endpoint": "unix:///tmp/foo.sock",
//!   "functions": [
//!     { "call_id": 1, "symbol": "add", "num_args": 2, "ret": "i32" }
//!   ]
//! }
//!
//! Wire protocol (little-endian, one request per connection):
//!   Request:  [call_id: u32][num_args: u32][arg0..argN: u64 each]
//!   Response: [result: u64][errno: i32]
//!
//! Only integer (i32/i64) scalar functions are supported. Float arguments
//! require XMM registers and are not handled by this initial implementation.

use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};

use anyhow::{anyhow, Result};
use serde::Deserialize;

// ---- Config ----

#[derive(Deserialize)]
struct ServerConfig {
    library: String,
    endpoint: String,
    functions: Vec<FunctionEntry>,
}

#[derive(Deserialize)]
struct FunctionEntry {
    call_id: u32,
    symbol: String,
    num_args: usize,
    ret: String, // "i32", "i64", or "void"
}

// ---- Function registry ----

struct LoadedFn {
    ptr: *mut libc::c_void,
    #[allow(dead_code)]
    num_args: usize,
    ret: String,
}

// SAFETY: function pointers remain valid for the lifetime of the loaded library handle,
// which lives for the entire process lifetime of this server.
unsafe impl Send for LoadedFn {}
unsafe impl Sync for LoadedFn {}

// ---- Native scalar call trampoline ----
//
// All scalar integer arguments are passed as i64. On the x86-64 System V ABI,
// integer arguments go into general-purpose registers (rdi, rsi, rdx, rcx, r8,
// r9). When the callee expects i32, it reads only the lower 32 bits of the
// register, so passing i64 is correct. A maximum of 6 integer arguments is
// supported (the six integer-class argument registers).
//
// Float arguments (f32/f64) use XMM registers and are not covered here.

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

    if ret == "void" {
        0
    } else {
        raw as u64
    }
}

// ---- Request handler ----

fn handle_request(stream: &mut UnixStream, registry: &HashMap<u32, LoadedFn>) -> Result<()> {
    let mut buf4 = [0u8; 4];

    stream.read_exact(&mut buf4)?;
    let call_id = u32::from_le_bytes(buf4);

    stream.read_exact(&mut buf4)?;
    let num_args = u32::from_le_bytes(buf4) as usize;

    let mut args = vec![0u64; num_args];
    for slot in &mut args {
        let mut buf8 = [0u8; 8];
        stream.read_exact(&mut buf8)?;
        *slot = u64::from_le_bytes(buf8);
    }

    let entry = registry
        .get(&call_id)
        .ok_or_else(|| anyhow!("unknown call_id {call_id}"))?;

    let result = unsafe { call_scalar_native(entry.ptr, &args, &entry.ret) };
    let errno_val = unsafe { *libc::__errno_location() } as i32;

    stream.write_all(&result.to_le_bytes())?;
    stream.write_all(&errno_val.to_le_bytes())?;
    stream.flush()?;

    Ok(())
}

// ---- Server loop ----

fn run(config_path: &str) -> Result<()> {
    let content = fs::read_to_string(config_path)?;
    let config: ServerConfig = serde_json::from_str(&content)?;

    let socket_path = config
        .endpoint
        .strip_prefix("unix://")
        .ok_or_else(|| anyhow!("invalid endpoint: {}", config.endpoint))?;

    // Load the shared library
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

    // Resolve symbols and build the call_id registry
    let mut registry: HashMap<u32, LoadedFn> = HashMap::new();
    for entry in &config.functions {
        let sym_cstr = CString::new(entry.symbol.as_str())?;
        let ptr = unsafe { libc::dlsym(lib_handle, sym_cstr.as_ptr()) };
        if ptr.is_null() {
            eprintln!("remote-server: symbol not found: {}", entry.symbol);
            continue;
        }
        registry.insert(
            entry.call_id,
            LoadedFn {
                ptr,
                num_args: entry.num_args,
                ret: entry.ret.clone(),
            },
        );
        println!(
            "remote-server: registered {} (call_id={})",
            entry.symbol, entry.call_id
        );
    }

    // Bind the Unix socket
    let _ = fs::remove_file(socket_path); // remove stale socket if any
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
