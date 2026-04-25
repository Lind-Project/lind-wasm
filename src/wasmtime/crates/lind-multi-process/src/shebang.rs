use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shebang {
    /// Interpreter path, e.g. "/usr/bin/env"
    pub interpreter: PathBuf,

    /// Optional single argument from shebang, e.g. "python3"
    ///
    /// Important: this is a single string, not shell-split.
    pub arg: Option<OsString>,
}

/// Parse a Linux-style shebang from `path`.
///
/// Returns:
/// - `Ok(Some(...))` if the file starts with a valid shebang
/// - `Ok(None)` if there is no shebang
/// - `Err(...)` on I/O errors
///
/// Linux-like behavior:
/// - reads only a small prefix of the file
/// - requires first two bytes to be `#!`
/// - interpreter is first non-whitespace token after `#!`
/// - the remainder of the line, after leading spaces/tabs, is one optional arg
pub fn parse_shebang(path: &Path) -> Result<Option<Shebang>> {
    // A small fixed buffer is enough for shebang parsing.
    // Linux uses a fixed-size exec buffer too.
    // Linux's shebang limit is 255 chars, let's also use that
    const BUF_SIZE: usize = 255;

    let mut file = File::open(path)?;
    let mut buf = [0u8; BUF_SIZE];
    let n = file.read(&mut buf)?;

    if n < 2 || &buf[..2] != b"#!" {
        return Ok(None);
    }

    let data = &buf[..n];

    // Find end of first line.
    let line_end = data.iter().position(|&b| b == b'\n').unwrap_or(data.len());

    let mut line = &data[2..line_end];

    // Trim trailing '\r' for CRLF files.
    if let Some(b'\r') = line.last().copied() {
        line = &line[..line.len() - 1];
    }

    // Skip spaces/tabs after "#!"
    line = trim_start_spaces_tabs(line);

    if line.is_empty() {
        return Ok(None);
    }

    // Interpreter path = first token up to space/tab
    let interp_end = line
        .iter()
        .position(|&b| b == b' ' || b == b'\t')
        .unwrap_or(line.len());

    let interpreter_bytes = &line[..interp_end];
    if interpreter_bytes.is_empty() {
        return Ok(None);
    }

    let interpreter = PathBuf::from(os_string_from_bytes(interpreter_bytes));

    // The rest of the line is the optional single argument.
    // Linux does not shell-split this.
    let rest = trim_start_spaces_tabs(&line[interp_end..]);

    let arg = if rest.is_empty() {
        None
    } else {
        Some(os_string_from_bytes(rest))
    };

    Ok(Some(Shebang { interpreter, arg }))
}

/// Build the argv that should be passed to the interpreter.
///
/// Resulting argv is:
///   [interpreter, optional_arg, original_args...]
pub fn build_shebang_argv(shebang: &Shebang, original_args: &Vec<String>) -> Result<Vec<String>> {
    let mut argv = Vec::with_capacity(2 + original_args.len() + usize::from(shebang.arg.is_some()));

    let interpreter = shebang.interpreter.to_str().ok_or_else(|| anyhow!(""))?;
    argv.push(interpreter.to_string());

    if let Some(arg) = &shebang.arg {
        let arg_str = arg.to_str().ok_or_else(|| anyhow!(""))?;
        argv.push(arg_str.to_string());
    }

    argv.extend(original_args.iter().cloned());

    Ok(argv)
}

fn os_string_from_bytes(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(bytes.to_vec())
}

fn trim_start_spaces_tabs(mut s: &[u8]) -> &[u8] {
    while let Some(&b) = s.first() {
        if b == b' ' || b == b'\t' {
            s = &s[1..];
        } else {
            break;
        }
    }
    s
}
