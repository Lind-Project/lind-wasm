use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use anyhow::{Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shebang {
    /// Interpreter path, e.g. "/usr/bin/env"
    pub interpreter: PathBuf,

    /// Optional single argument from shebang, e.g. "python3"
    ///
    /// Important: this is a single string, not shell-split.
    pub arg: Option<OsString>,
}

/// Parse a Linux-style shebang from a file prefix.
///
/// Returns:
/// - `Ok(Some(...))` if the file starts with a valid shebang
/// - `Ok(None)` if there is no shebang
///
/// Linux-like behavior:
/// - reads only a small prefix of the file
/// - requires first two bytes to be `#!`
/// - interpreter is first non-whitespace token after `#!`
/// - the remainder of the line, after leading spaces/tabs, is one optional arg
pub fn parse_shebang(bytes: &[u8]) -> Result<Option<Shebang>> {
    let data = &bytes[..bytes.len().min(SHEBANG_BUF_SIZE)];

    if data.len() < 2 || &data[..2] != b"#!" {
        return Ok(None);
    }

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

// A small fixed buffer is enough for shebang parsing.
// Linux uses a fixed-size exec buffer too. Linux's shebang limit is 255 chars,
// so keep the parser bounded the same way even when the caller has full bytes.
const SHEBANG_BUF_SIZE: usize = 255;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_interpreter_without_arg() {
        let shebang = parse_shebang(b"#!/bin/sh\necho ok\n").unwrap().unwrap();

        assert_eq!(shebang.interpreter.to_str(), Some("/bin/sh"));
        assert_eq!(shebang.arg, None);
    }

    #[test]
    fn parses_single_optional_arg() {
        let shebang = parse_shebang(b"#!/usr/bin/env python3 -O\n")
            .unwrap()
            .unwrap();

        assert_eq!(shebang.interpreter.to_str(), Some("/usr/bin/env"));
        assert_eq!(
            shebang.arg.as_ref().and_then(|arg| arg.to_str()),
            Some("python3 -O")
        );
    }

    #[test]
    fn returns_none_without_shebang() {
        assert!(parse_shebang(b"\0asm").unwrap().is_none());
    }
}
