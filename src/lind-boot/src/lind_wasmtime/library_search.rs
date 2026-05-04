use std::path::{Path, PathBuf};
use sysdefs::constants::lind_platform_const::{DEFAULT_LIBRARY_SEARCH_PATHS, LD_LIBRARY_PATH_VAR};
use wasmtime_lind_common::LindEnviron;

/// Locate a shared library binary on the (chroot'd) filesystem.
///
/// Search order:
/// 1. Each directory listed in the guest `LD_LIBRARY_PATH` environment variable.
/// 2. The platform default directories (`/lib`, `/usr/lib`).
///
/// Returns the path of the first file found, or `None` if the library is not
/// found in any search location.
pub fn find_library_path(lib_name: &str, environ: &LindEnviron) -> Option<PathBuf> {
    let ld_path = environ.get_var(LD_LIBRARY_PATH_VAR);

    if let Some(ld_path) = ld_path {
        for dir in ld_path.split(':') {
            let candidate = Path::new(dir).join(lib_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    for dir in DEFAULT_LIBRARY_SEARCH_PATHS {
        let candidate = Path::new(dir).join(lib_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}
