//! Remote library call routing and RPC wire protocol.
//!
//! Used by:
//! - wasmtime's `instance_dylink` wrapper to look up routing decisions and
//!   send RPC calls to a remote server
//! - `lind-remote-server` to read requests from the wire and send responses back

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use serde::Deserialize;

// ---- Argument metadata types ----

/// Direction of a pointer argument relative to the call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
    InOut,
}

/// How the size of a pointer argument is determined.
#[derive(Debug, Clone)]
pub enum PtrSizeSpec {
    /// Size comes from the scalar argument at this index.
    Arg(usize),
    /// Scan WASM memory for '\0'; size includes the terminator. Capped at 4096 bytes.
    NullTerminated,
    /// Size equals the already-computed alloc_size of the ptr arg at this overall arg index.
    /// Used for Out args whose size is determined by another pointer argument (e.g. strcpy dest).
    SameAsPtrArg(usize),
}

/// Per-argument specification for a remotely-dispatched function.
#[derive(Debug, Clone)]
pub enum ArgSpec {
    Scalar,
    Ptr { direction: Direction, size: PtrSizeSpec },
}

/// Argument metadata for a remotely-dispatched function.
#[derive(Debug, Clone)]
pub struct FunctionMeta {
    pub args: Vec<ArgSpec>,
}

impl FunctionMeta {
    pub fn has_ptrs(&self) -> bool {
        self.args.iter().any(|a| matches!(a, ArgSpec::Ptr { .. }))
    }
}

// ---- Routing config JSON structures ----

#[derive(Deserialize)]
struct RoutingConfigFile {
    default_route: String,
    #[serde(default)]
    remotes: HashMap<String, RemoteEndpointConfig>,
    #[serde(default)]
    routes: Vec<RouteEntryConfig>,
}

#[derive(Deserialize)]
struct RemoteEndpointConfig {
    endpoint: String,
}

#[derive(Deserialize)]
struct ArgSpecConfig {
    #[serde(rename = "type", default = "default_arg_type")]
    ty: String,
    direction: Option<String>,
    size_arg: Option<usize>,
    #[serde(default)]
    null_terminated: bool,
    same_as_arg: Option<usize>,
}

fn default_arg_type() -> String {
    "scalar".to_string()
}

#[derive(Deserialize)]
struct RouteEntryConfig {
    symbol: String,
    route: String,
    remote: Option<String>,
    call_id: Option<u32>,
    #[serde(default)]
    args: Vec<ArgSpecConfig>,
}

// ---- Resolved routing decisions ----

#[derive(Debug)]
pub enum RouteDecision {
    Local,
    Remote { call_id: u32, endpoint: String },
}

struct RoutingState {
    default_decision: RouteDecision,
    route_table: HashMap<String, RouteDecision>,
    meta_table: HashMap<String, FunctionMeta>,
}

static ROUTING_STATE: OnceLock<RoutingState> = OnceLock::new();

fn build_routing_state() -> RoutingState {
    let empty = || RoutingState {
        default_decision: RouteDecision::Local,
        route_table: HashMap::new(),
        meta_table: HashMap::new(),
    };

    let config_path = match std::env::var("LIND_REMOTE_CONFIG") {
        Ok(p) => p,
        Err(_) => return empty(),
    };
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("remote-lib: cannot read config {config_path}: {e}");
            return empty();
        }
    };
    let file: RoutingConfigFile = match serde_json::from_str(&content) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("remote-lib: cannot parse config: {e}");
            return empty();
        }
    };

    let _ = &file.default_route; // only "local" default supported for now

    let mut route_table = HashMap::new();
    let mut meta_table = HashMap::new();

    for entry in file.routes {
        let decision = if entry.route == "remote" {
            let remote_name = entry.remote.as_deref().unwrap_or("");
            let endpoint = file
                .remotes
                .get(remote_name)
                .map(|r| r.endpoint.clone())
                .unwrap_or_default();
            let call_id = entry.call_id.unwrap_or(0);
            RouteDecision::Remote { call_id, endpoint }
        } else {
            RouteDecision::Local
        };

        if !entry.args.is_empty() {
            let specs: Vec<ArgSpec> = entry
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
                        let size = if a.null_terminated {
                            PtrSizeSpec::NullTerminated
                        } else if let Some(j) = a.same_as_arg {
                            PtrSizeSpec::SameAsPtrArg(j)
                        } else {
                            PtrSizeSpec::Arg(a.size_arg.unwrap_or(0))
                        };
                        ArgSpec::Ptr { direction, size }
                    } else {
                        ArgSpec::Scalar
                    }
                })
                .collect();
            meta_table.insert(entry.symbol.clone(), FunctionMeta { args: specs });
        }

        route_table.insert(entry.symbol, decision);
    }

    RoutingState {
        default_decision: RouteDecision::Local,
        route_table,
        meta_table,
    }
}

/// Look up the routing decision for `symbol`. Returns `Local` if no config
/// was loaded or the symbol has no explicit route.
pub fn get_route(symbol: &str) -> &'static RouteDecision {
    let state = ROUTING_STATE.get_or_init(build_routing_state);
    let decision = state.route_table.get(symbol).unwrap_or(&state.default_decision);
    println!("[debug] routing decision for {}: {:?}", symbol, decision);
    decision
}

/// Look up argument metadata for `symbol`, if present in the routing config.
pub fn get_meta(symbol: &str) -> Option<&'static FunctionMeta> {
    let state = ROUTING_STATE.get_or_init(build_routing_state);
    state.meta_table.get(symbol)
}

// ---- Wire protocol ----
//
// Scalar request  (little-endian): [call_id: u32][num_args: u32][arg0..N: u64]
// Scalar response (little-endian): [result: u64][errno: i32]
//
// Extended request (ptr args):
//   [call_id: u32][num_args: u32][arg0..N: u64]  <- ptr positions = 0
//   [num_ptr_args: u32]
//   [for each Ptr arg in declaration order:
//     [alloc_size: u32]
//     [data: alloc_size bytes]   <- omitted for Out-direction args
//   ]
//
// Extended response (ptr args):
//   [result: u64][errno: i32]
//   [num_out_bufs: u32]
//   [for each Out/InOut arg in declaration order:
//     [size: u32][data: size bytes]
//   ]

fn parse_unix_path(endpoint: &str) -> Option<&str> {
    endpoint.strip_prefix("unix://")
}

// ---- Scalar API (backward-compatible) ----

/// Client: open a connection to `endpoint`, send `(call_id, args)`,
/// and return `(result_u64, errno)`.
pub fn rpc_call(endpoint: &str, call_id: u32, args: &[u64]) -> Result<(u64, i32)> {
    let path = parse_unix_path(endpoint)
        .ok_or_else(|| anyhow!("invalid remote endpoint: {endpoint}"))?;

    let mut stream = UnixStream::connect(path)
        .map_err(|e| anyhow!("remote-lib: connect to {path}: {e}"))?;

    stream.write_all(&call_id.to_le_bytes())?;
    stream.write_all(&(args.len() as u32).to_le_bytes())?;
    for &arg in args {
        stream.write_all(&arg.to_le_bytes())?;
    }
    stream.flush()?;

    let mut result_buf = [0u8; 8];
    let mut errno_buf = [0u8; 4];
    stream.read_exact(&mut result_buf)?;
    stream.read_exact(&mut errno_buf)?;

    Ok((
        u64::from_le_bytes(result_buf),
        i32::from_le_bytes(errno_buf),
    ))
}

/// Server: read one scalar request from an accepted stream.
/// Returns `(call_id, args)`.
pub fn read_request(stream: &mut UnixStream) -> Result<(u32, Vec<u64>)> {
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

    Ok((call_id, args))
}

/// Server: write `(result, errno)` back to the caller.
pub fn write_response(stream: &mut UnixStream, result: u64, errno: i32) -> Result<()> {
    stream.write_all(&result.to_le_bytes())?;
    stream.write_all(&errno.to_le_bytes())?;
    stream.flush()?;
    Ok(())
}

// ---- Pointer-aware API ----

/// One pointer argument payload for an extended RPC request.
pub struct PtrBuf {
    pub direction: Direction,
    /// Bytes to allocate on the server. For In/InOut this equals `data.len()`.
    pub alloc_size: u32,
    /// Buffer contents. Empty for `Out`-direction pointers.
    pub data: Vec<u8>,
}

/// Client: send an extended RPC with pointer arguments.
///
/// `args` — scalar u64 values; pointer positions must be set to 0.
/// `ptr_bufs` — one entry per `Ptr` arg in declaration order.
///
/// Returns `(result, errno, out_bufs)` where `out_bufs` contains one `Vec<u8>`
/// per `Out`/`InOut` pointer in declaration order.
pub fn rpc_call_with_ptrs(
    endpoint: &str,
    call_id: u32,
    args: &[u64],
    ptr_bufs: &[PtrBuf],
) -> Result<(u64, i32, Vec<Vec<u8>>)> {
    let path = parse_unix_path(endpoint)
        .ok_or_else(|| anyhow!("invalid remote endpoint: {endpoint}"))?;

    let mut stream = UnixStream::connect(path)
        .map_err(|e| anyhow!("remote-lib: connect to {path}: {e}"))?;

    // Scalar header
    stream.write_all(&call_id.to_le_bytes())?;
    stream.write_all(&(args.len() as u32).to_le_bytes())?;
    for &a in args {
        stream.write_all(&a.to_le_bytes())?;
    }

    // Ptr section
    stream.write_all(&(ptr_bufs.len() as u32).to_le_bytes())?;
    for buf in ptr_bufs {
        stream.write_all(&buf.alloc_size.to_le_bytes())?;
        if buf.direction != Direction::Out {
            stream.write_all(&buf.data)?;
        }
    }
    stream.flush()?;

    // Response
    let mut r8 = [0u8; 8];
    let mut r4 = [0u8; 4];
    stream.read_exact(&mut r8)?;
    let result = u64::from_le_bytes(r8);
    stream.read_exact(&mut r4)?;
    let errno = i32::from_le_bytes(r4);

    stream.read_exact(&mut r4)?;
    let num_out = u32::from_le_bytes(r4) as usize;
    let mut out_bufs = Vec::with_capacity(num_out);
    for _ in 0..num_out {
        stream.read_exact(&mut r4)?;
        let size = u32::from_le_bytes(r4) as usize;
        let mut data = vec![0u8; size];
        stream.read_exact(&mut data)?;
        out_bufs.push(data);
    }

    Ok((result, errno, out_bufs))
}

/// Server: read the call_id from a stream without consuming the rest.
/// Used to dispatch before reading args.
pub fn read_call_id(stream: &mut UnixStream) -> Result<u32> {
    let mut buf4 = [0u8; 4];
    stream.read_exact(&mut buf4)?;
    Ok(u32::from_le_bytes(buf4))
}

/// Server: read scalar args (num_args + args) from a stream.
/// Call after `read_call_id`.
pub fn read_scalar_args(stream: &mut UnixStream) -> Result<Vec<u64>> {
    let mut buf4 = [0u8; 4];
    stream.read_exact(&mut buf4)?;
    let num_args = u32::from_le_bytes(buf4) as usize;
    let mut args = vec![0u64; num_args];
    for slot in &mut args {
        let mut buf8 = [0u8; 8];
        stream.read_exact(&mut buf8)?;
        *slot = u64::from_le_bytes(buf8);
    }
    Ok(args)
}

/// One received pointer payload on the server side.
pub struct ReceivedPtrBuf {
    pub direction: Direction,
    /// Declared allocation size.
    pub alloc_size: u32,
    /// Received data. Empty for `Out`-direction pointers.
    pub data: Vec<u8>,
}

/// Server: read the ptr section from a stream after scalar args have been read.
///
/// `ptr_directions` must list the direction of each `Ptr` arg in declaration order.
pub fn read_ptr_sections(
    stream: &mut UnixStream,
    ptr_directions: &[Direction],
) -> Result<Vec<ReceivedPtrBuf>> {
    let mut buf4 = [0u8; 4];
    stream.read_exact(&mut buf4)?;
    let num_ptr = u32::from_le_bytes(buf4) as usize;

    let mut ptr_bufs = Vec::with_capacity(num_ptr);
    for i in 0..num_ptr {
        stream.read_exact(&mut buf4)?;
        let alloc_size = u32::from_le_bytes(buf4);
        let direction = ptr_directions.get(i).cloned().unwrap_or(Direction::In);
        let data = if direction != Direction::Out {
            let mut d = vec![0u8; alloc_size as usize];
            stream.read_exact(&mut d)?;
            d
        } else {
            Vec::new()
        };
        ptr_bufs.push(ReceivedPtrBuf {
            direction,
            alloc_size,
            data,
        });
    }

    Ok(ptr_bufs)
}

/// Server: write an extended response with output pointer sections.
pub fn write_response_with_ptrs(
    stream: &mut UnixStream,
    result: u64,
    errno: i32,
    out_bufs: &[Vec<u8>],
) -> Result<()> {
    stream.write_all(&result.to_le_bytes())?;
    stream.write_all(&errno.to_le_bytes())?;
    stream.write_all(&(out_bufs.len() as u32).to_le_bytes())?;
    for buf in out_bufs {
        stream.write_all(&(buf.len() as u32).to_le_bytes())?;
        stream.write_all(buf)?;
    }
    stream.flush()?;
    Ok(())
}
