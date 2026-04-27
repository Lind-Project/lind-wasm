use crate::prelude::*;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::OnceLock;

use anyhow::anyhow;
use serde::Deserialize;

use crate::{FuncType, Val, ValType};

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

// ---- RPC client (scalar-only) ----

fn parse_unix_path(endpoint: &str) -> Option<&str> {
    endpoint.strip_prefix("unix://")
}

fn val_to_u64(v: &Val) -> u64 {
    match v {
        Val::I32(i) => *i as u64,
        Val::I64(i) => *i as u64,
        Val::F32(bits) => *bits as u64,
        Val::F64(bits) => *bits,
        _ => 0,
    }
}

fn u64_to_val(raw: u64, ty: ValType) -> Result<Val> {
    Ok(match ty {
        ValType::I32 => Val::I32(raw as i32),
        ValType::I64 => Val::I64(raw as i64),
        ValType::F32 => Val::F32(raw as u32),
        ValType::F64 => Val::F64(raw),
        other => return Err(anyhow!("unsupported return type for remote call: {other}")),
    })
}

/// Send a scalar-only RPC call to a remote server and write results back.
///
/// Wire format (little-endian):
///   Request:  [call_id: u32][num_args: u32][arg0..argN: u64 each]
///   Response: [result: u64][errno: i32]
///
/// A new TCP/Unix connection is opened per call. Only the first result
/// slot is populated; functions with multiple returns are unsupported.
pub fn rpc_call_scalar(
    endpoint: &str,
    call_id: u32,
    params: &[Val],
    results: &mut [Val],
    func_ty: &FuncType,
) -> Result<()> {
    let path = parse_unix_path(endpoint)
        .ok_or_else(|| anyhow!("invalid remote endpoint: {endpoint}"))?;

    let mut stream = UnixStream::connect(path)
        .map_err(|e| anyhow!("remote-lib: connect to {path}: {e}"))?;

    // Send request
    stream.write_all(&call_id.to_le_bytes())?;
    stream.write_all(&(params.len() as u32).to_le_bytes())?;
    for param in params {
        stream.write_all(&val_to_u64(param).to_le_bytes())?;
    }
    stream.flush()?;

    // Receive response
    let mut result_buf = [0u8; 8];
    let mut errno_buf = [0u8; 4];
    stream.read_exact(&mut result_buf)?;
    stream.read_exact(&mut errno_buf)?;

    let result_u64 = u64::from_le_bytes(result_buf);
    // TODO: propagate errno back into WASM errno (requires access to caller memory)
    let _errno_val = i32::from_le_bytes(errno_buf);

    // Write result slots from the remote return value
    for (slot, ty) in results.iter_mut().zip(func_ty.results()) {
        *slot = u64_to_val(result_u64, ty)?;
    }

    Ok(())
}
