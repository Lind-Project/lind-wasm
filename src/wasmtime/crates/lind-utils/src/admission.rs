//! Module admission.
//!
//! A precompiled module (`.cwasm`) is native code: lind deserializes it and
//! runs it with every right the runtime has, so whoever supplies the file
//! supplies code. An embedder that must run only modules it vetted (TriSeal,
//! where the host supplies them) registers a check with [`register`]. lind
//! calls [`check`] with the bytes of every module before it deserializes or
//! compiles them, and refuses the module if the check fails.
//!
//! While a check is registered, lind loads a module file by reading it into
//! memory, not by mapping it, and keeps wasmtime's compilation cache off:
//! the bytes checked are then the code that runs, and no compiled code is
//! read from the cache directory.

use anyhow::{Result, bail};
use std::sync::OnceLock;

/// Decides whether lind may load a module: given its name (a path, or the
/// name the embedder gave the main module), its bytes and whether they are
/// a precompiled module, returns `Err` with the reason to refuse it.
pub type AdmissionCheck = Box<dyn Fn(&str, &[u8], bool) -> Result<(), String> + Send + Sync>;

static CHECK: OnceLock<AdmissionCheck> = OnceLock::new();

/// Check every module with `check` from now on. Only the first check
/// registered is kept; register it before lind starts.
pub fn register(check: AdmissionCheck) {
    let _ = CHECK.set(check);
}

/// Whether a check is registered.
pub fn is_registered() -> bool {
    CHECK.get().is_some()
}

/// Run the registered check, if any, on the module `name` made of `bytes`.
pub fn check(bytes: &[u8], precompiled: bool, name: &str) -> Result<()> {
    match CHECK.get() {
        Some(check) => match check(name, bytes, precompiled) {
            Ok(()) => Ok(()),
            Err(reason) => bail!("module {name} refused: {reason}"),
        },
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The check is process-wide, so this is the only test that registers one.
    #[test]
    fn registered_check_decides() {
        assert!(check(b"anything", false, "before").is_ok());
        assert!(!is_registered());
        register(Box::new(|_, bytes, precompiled| {
            if precompiled && bytes.starts_with(b"bad") {
                Err("not allowed".to_string())
            } else {
                Ok(())
            }
        }));
        assert!(is_registered());
        assert!(check(b"good", true, "good.cwasm").is_ok());
        assert!(check(b"bad", false, "bad.wasm").is_ok());
        let err = check(b"bad", true, "bad.cwasm").unwrap_err().to_string();
        assert_eq!(err, "module bad.cwasm refused: not allowed");
        // A second check does not replace the first.
        register(Box::new(|_, _, _| Ok(())));
        assert!(check(b"bad", true, "bad.cwasm").is_err());
    }
}
