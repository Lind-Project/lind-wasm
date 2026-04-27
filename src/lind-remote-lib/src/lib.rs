//! Remote library call routing and RPC wire protocol.
//!
//! Used by:
//! - wasmtime's `instance_dylink` wrapper to look up routing decisions and
//!   send scalar RPC calls to a remote server
//! - `lind-remote-server` to read requests from the wire and send responses back

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use serde::Deserialize;

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
struct RouteEntryConfig {
    symbol: String,
    route: String,
    remote: Option<String>,
    call_id: Option<u32>,
}

// ---- Resolved routing decisions ----

#[derive(Debug)]
pub enum RouteDecision {
    Local,
    Remote { call_id: u32, endpoint: String },
}

// (default_decision, per-symbol table)
static ROUTING_TABLE: OnceLock<(RouteDecision, HashMap<String, RouteDecision>)> =
    OnceLock::new();

fn build_routing_table() -> (RouteDecision, HashMap<String, RouteDecision>) {
    let config_path = match std::env::var("LIND_REMOTE_CONFIG") {
        Ok(p) => p,
        Err(_) => return (RouteDecision::Local, HashMap::new()),
    };
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("remote-lib: cannot read config {config_path}: {e}");
            return (RouteDecision::Local, HashMap::new());
        }
    };
    let file: RoutingConfigFile = match serde_json::from_str(&content) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("remote-lib: cannot parse config: {e}");
            return (RouteDecision::Local, HashMap::new());
        }
    };

    let _ = &file.default_route; // only "local" default supported for now

    let mut table = HashMap::new();
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
        table.insert(entry.symbol, decision);
    }

    (RouteDecision::Local, table)
}

/// Look up the routing decision for `symbol`. Returns `Local` if no config
/// was loaded or the symbol has no explicit route.
pub fn get_route(symbol: &str) -> &'static RouteDecision {
    let (default, table) = ROUTING_TABLE.get_or_init(build_routing_table);
    table.get(symbol).unwrap_or(default)
}

// ---- Wire protocol (shared between client and server) ----
//
// Request  (little-endian): [call_id: u32][num_args: u32][arg0..argN: u64 each]
// Response (little-endian): [result: u64][errno: i32]

fn parse_unix_path(endpoint: &str) -> Option<&str> {
    endpoint.strip_prefix("unix://")
}

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

/// Server: read one request from an already-accepted stream.
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
