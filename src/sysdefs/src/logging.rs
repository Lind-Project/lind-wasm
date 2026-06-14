//! Logging and diagnostic system for Lind.
//!
//! # Two distinct APIs
//!
//! ## `lind_log!`
//! General-purpose diagnostic output. Never panics. Always writes to the
//! configured destination when the `lind-logging` Cargo feature is enabled
//! and the message's category is in the active category set.
//!
//! ## `lind_debug_panic!`
//! Soft panic for unexpected but potentially survivable conditions — use when
//! something that *should never happen* is detected but continued execution
//! may still be possible. Its behavior is controlled by [`PanicBehavior`].
//!
//! Rust's built-in `panic!` remains reserved for truly unrecoverable failures
//! where continued execution is impossible.
//!
//! # Compile-time gating
//!
//! Both macros expand to nothing when the `lind-logging` Cargo feature is
//! absent. Formatting arguments are **not evaluated** in that case.
//!
//! ```bash
//! # Logging present (default when using `make lind-boot`)
//! cargo build --features lind-logging
//!
//! # Maximum-performance / benchmark build — zero logging overhead
//! cargo build --release --no-default-features
//! ```

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// ---------------------------------------------------------------------------
// Log categories
// ---------------------------------------------------------------------------

/// A log category used to route and filter [`lind_log!`] messages.
///
/// `lind_debug_panic!` does not use categories — it always fires regardless
/// of the active category set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogCategory {
    /// Default category for uncategorized Lind diagnostics.
    /// Used when `lind_log!` is called without an explicit category.
    Default,
    /// Dynamic linking: dlopen/dlsym/dlclose, GOT updates, symbol resolution,
    /// library replay, and related loader/runtime behavior.
    DYLINK,
    /// ThreeI: syscall routing, handler registration, inter-cage call routing,
    /// and library interposition logic that reuses ThreeI-style APIs.
    THREEI,
}

impl LogCategory {
    fn as_str(self) -> &'static str {
        match self {
            LogCategory::Default => "Default",
            LogCategory::DYLINK => "DYLINK",
            LogCategory::THREEI => "THREEI",
        }
    }
}

const BIT_DEFAULT: u8 = 0b001;
const BIT_DYLINK: u8 = 0b010;
const BIT_THREEI: u8 = 0b100;

/// A set of enabled [`LogCategory`] values backed by a bitmask.
pub struct LogCategorySet(u8);

impl LogCategorySet {
    /// All categories enabled.
    pub fn all() -> Self {
        Self(BIT_DEFAULT | BIT_DYLINK | BIT_THREEI)
    }

    /// No categories enabled.
    pub fn none() -> Self {
        Self(0)
    }

    /// Only `Default` category enabled.
    pub fn default_only() -> Self {
        Self(BIT_DEFAULT)
    }

    /// Returns `true` if `category` is in this set.
    pub fn contains(&self, category: LogCategory) -> bool {
        let bit = match category {
            LogCategory::Default => BIT_DEFAULT,
            LogCategory::DYLINK => BIT_DYLINK,
            LogCategory::THREEI => BIT_THREEI,
        };
        self.0 & bit != 0
    }

    /// Parse a comma-separated list of category names (case-insensitive).
    ///
    /// Accepted values: `all`, `none`, `default`, `dylink`, `threei`, and
    /// any comma-separated combination of the individual names.
    ///
    /// Returns an error for unrecognised names.
    pub fn from_csv(s: &str) -> Result<Self, LindLoggerInitError> {
        let lower = s.trim().to_lowercase();
        if lower == "all" {
            return Ok(Self::all());
        }
        if lower == "none" {
            return Ok(Self::none());
        }
        let mut bits: u8 = 0;
        for part in lower.split(',') {
            match part.trim() {
                "default" => bits |= BIT_DEFAULT,
                "dylink" => bits |= BIT_DYLINK,
                "threei" => bits |= BIT_THREEI,
                other => {
                    return Err(LindLoggerInitError::InvalidConfig(format!(
                        "unknown log category: '{}'",
                        other
                    )));
                }
            }
        }
        Ok(Self(bits))
    }
}

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Where log output is written.
pub enum LogOutput {
    Stdout,
    Stderr,
    /// Append to the file at the given path (created if absent).
    File(PathBuf),
    /// Discard ordinary log output.
    ///
    /// Suppresses `lind_log!` output.  Does **not** by itself decide whether
    /// `lind_debug_panic!` panics — that is controlled by [`PanicBehavior`].
    None,
}

/// Controls the behavior of `lind_debug_panic!`.
pub enum PanicBehavior {
    /// Log the message then call `panic!`. Default when uninitialized.
    PanicAndExit,
    /// Log the message and return normally (best-effort continuation).
    LogOnly,
    /// Do nothing and return immediately (silent no-op).
    NoAction,
}

/// Runtime configuration for the Lind logger.
pub struct LindLoggerConfig {
    pub output: LogOutput,
    pub panic_behavior: PanicBehavior,
    pub enabled_categories: LogCategorySet,
}

impl Default for LindLoggerConfig {
    fn default() -> Self {
        let log_path = match std::env::var("LIND_WASM_ROOT") {
            Ok(root) => PathBuf::from(root).join("LIND.log"),
            Err(_) => PathBuf::from("/tmp/LIND.log"),
        };
        Self {
            output: LogOutput::File(log_path),
            panic_behavior: PanicBehavior::PanicAndExit,
            enabled_categories: LogCategorySet::default_only(),
        }
    }
}

/// Errors returned by [`init_lind_logger`].
pub enum LindLoggerInitError {
    /// `init_lind_logger` was already called; the first call wins.
    AlreadyInitialized,
    /// A file output destination could not be opened.
    Io(std::io::Error),
    /// A configuration value (e.g. from an env var) was not recognised.
    InvalidConfig(String),
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

enum LogWriter {
    Stdout,
    Stderr,
    File(File),
    None,
}

impl LogWriter {
    fn write_line(&mut self, line: &str) {
        match self {
            LogWriter::Stdout => println!("{}", line),
            LogWriter::Stderr => eprintln!("{}", line),
            LogWriter::File(f) => {
                let _ = writeln!(f, "{}", line);
            }
            LogWriter::None => {}
        }
    }

    fn is_none(&self) -> bool {
        matches!(self, LogWriter::None)
    }
}

struct LindLogger {
    writer: Mutex<LogWriter>,
    panic_behavior: PanicBehavior,
    enabled_categories: LogCategorySet,
}

static LIND_LOGGER: OnceLock<LindLogger> = OnceLock::new();

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

/// Initialize the global Lind logger.
///
/// Must be called once at startup before any log output is emitted.
/// **First call wins** — subsequent calls return [`LindLoggerInitError::AlreadyInitialized`]
/// and leave the existing configuration unchanged.
///
/// If this is never called and `lind-logging` is enabled, the defaults are:
/// `$LIND_WASM_ROOT/LIND.log` (or `/tmp/LIND.log` if unset), `PanicAndExit` behavior,
/// `Default` category only.
pub fn init_lind_logger(config: LindLoggerConfig) -> Result<(), LindLoggerInitError> {
    let writer = match config.output {
        LogOutput::Stdout => LogWriter::Stdout,
        LogOutput::Stderr => LogWriter::Stderr,
        LogOutput::None => LogWriter::None,
        LogOutput::File(path) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(LindLoggerInitError::Io)?;
            LogWriter::File(file)
        }
    };
    LIND_LOGGER
        .set(LindLogger {
            writer: Mutex::new(writer),
            panic_behavior: config.panic_behavior,
            enabled_categories: config.enabled_categories,
        })
        .map_err(|_| LindLoggerInitError::AlreadyInitialized)
}

/// Build a [`LindLoggerConfig`] from environment variables.
///
/// | Variable | Values | Default |
/// |---|---|---|
/// | `LIND_LOG_OUTPUT` | `stdout`, `stderr`, `none`, `file:/path`, `/path` | `stderr` |
/// | `LIND_LOG_CATEGORIES` | `all`, `none`, `general`, `dylink`, `threei`, comma-separated | `all` |
/// | `LIND_DEBUG_PANIC` | `panic`/`panic-and-exit`, `log`/`log-only`, `none`/`no-action` | `panic-and-exit` |
pub fn config_from_env() -> Result<LindLoggerConfig, LindLoggerInitError> {
    let mut config = LindLoggerConfig::default();
    if let Ok(val) = std::env::var("LIND_LOG_OUTPUT") {
        config.output = parse_log_output(&val)?;
    }
    if let Ok(val) = std::env::var("LIND_LOG_CATEGORIES") {
        config.enabled_categories = LogCategorySet::from_csv(&val)?;
    }
    if let Ok(val) = std::env::var("LIND_DEBUG_PANIC") {
        config.panic_behavior = parse_panic_behavior(&val)?;
    }
    Ok(config)
}

fn parse_log_output(s: &str) -> Result<LogOutput, LindLoggerInitError> {
    match s.trim() {
        "stdout" => Ok(LogOutput::Stdout),
        "stderr" => Ok(LogOutput::Stderr),
        "none" => Ok(LogOutput::None),
        other => {
            let path = other.strip_prefix("file:").unwrap_or(other);
            Ok(LogOutput::File(PathBuf::from(path)))
        }
    }
}

fn parse_panic_behavior(s: &str) -> Result<PanicBehavior, LindLoggerInitError> {
    match s.trim() {
        "panic" | "panic-and-exit" => Ok(PanicBehavior::PanicAndExit),
        "log" | "log-only" => Ok(PanicBehavior::LogOnly),
        "none" | "no-action" => Ok(PanicBehavior::NoAction),
        other => Err(LindLoggerInitError::InvalidConfig(format!(
            "unknown LIND_DEBUG_PANIC value: '{}' (expected: panic, log, none)",
            other
        ))),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn write_to_logger(line: &str) {
    match LIND_LOGGER.get() {
        Some(logger) => {
            if let Ok(mut w) = logger.writer.lock() {
                w.write_line(line);
            }
            // best-effort: ignore lock poison
        }
        None => eprintln!("{}", line),
    }
}

// ---------------------------------------------------------------------------
// Public internal functions (called by the macros)
// ---------------------------------------------------------------------------

/// Returns `true` if messages for `category` should be emitted.
///
/// When the logger is uninitialized all categories are enabled (default).
/// Called by the `lind_log!` and `lind_debug_panic!` macros before
/// evaluating format arguments.
pub fn category_enabled(category: LogCategory) -> bool {
    match LIND_LOGGER.get() {
        Some(logger) => logger.enabled_categories.contains(category),
        None => true,
    }
}

/// Write a log line.  Called by the `lind_log!` macro.
///
/// Output is skipped entirely when [`LogOutput::None`] is configured, but
/// the category check in the macro already prevents reaching this function
/// for disabled categories.
pub fn log(
    category: LogCategory,
    args: fmt::Arguments<'_>,
    file: &'static str,
    line: u32,
    module: &'static str,
) {
    match LIND_LOGGER.get() {
        Some(logger) => {
            if let Ok(writer) = logger.writer.lock() {
                if writer.is_none() {
                    return; // LogOutput::None — skip formatting
                }
            }
            let line_str = format!(
                "[LIND][{}][{}:{} {}] {}",
                category.as_str(),
                file,
                line,
                module,
                args
            );
            write_to_logger(&line_str);
        }
        None => {
            // Uninitialized: fall back to stderr
            eprintln!(
                "[LIND][{}][{}:{} {}] {}",
                category.as_str(),
                file,
                line,
                module,
                args
            );
        }
    }
}

/// Execute the soft-panic behavior.  Called by the `lind_debug_panic!` macro.
///
/// Does **not** use log categories — `lind_debug_panic!` always fires regardless
/// of the active [`LogCategorySet`].  [`LogOutput::None`] suppresses the log
/// line but does **not** suppress the `panic!` in `PanicAndExit` mode; this
/// lets CI environments silence ordinary log noise while still detecting
/// unexpected conditions as test failures.
///
/// Behavior depends on the [`PanicBehavior`] in the active configuration:
///
/// - [`PanicBehavior::PanicAndExit`]: logs then calls `panic!`.
/// - [`PanicBehavior::LogOnly`]: logs and returns normally.
/// - [`PanicBehavior::NoAction`]: returns immediately without logging.
///
/// When uninitialized, defaults to `PanicAndExit`.
///
/// Call sites **must always** supply explicit fallback control flow
/// (a `return`, a default value, etc.) because this function may return
/// normally in `LogOnly` and `NoAction` modes.
pub fn debug_panic(args: fmt::Arguments<'_>, file: &'static str, line: u32, module: &'static str) {
    let behavior = LIND_LOGGER
        .get()
        .map(|l| match &l.panic_behavior {
            PanicBehavior::PanicAndExit => 0u8,
            PanicBehavior::LogOnly => 1,
            PanicBehavior::NoAction => 2,
        })
        .unwrap_or(0);

    match behavior {
        2 => {} // NoAction
        1 => {
            // LogOnly: write and return
            let line_str = format!(
                "[LIND][DEBUG PANIC continuing][{}:{} {}] {}",
                file, line, module, args
            );
            write_to_logger(&line_str);
        }
        _ => {
            // PanicAndExit: render to String first so we can both log and panic.
            // Note: write_to_logger is a no-op when LogOutput::None, but the
            // panic! still fires — intentional for CI environments.
            let msg = fmt::format(args);
            let line_str = format!("[LIND][DEBUG PANIC][{}:{} {}] {}", file, line, module, msg);
            write_to_logger(&line_str);
            panic!("{}", line_str);
        }
    }
}

// ---------------------------------------------------------------------------
// Public macros
// ---------------------------------------------------------------------------

/// Log a formatted diagnostic message to the configured output.
///
/// Expands to nothing when the `lind-logging` Cargo feature is disabled.
/// Formatting arguments are **not evaluated** in that case.
///
/// # Usage
///
/// ```rust
/// use sysdefs::lind_log;
///
/// // Default category (General)
/// lind_log!("cage {} started", 42u64);
///
/// // Explicit category
/// lind_log!(DYLINK, "resolved symbol {} at 0x{:x}", "malloc", 0x1000u64);
/// lind_log!(THREEI, "registered handler for call {}", 7u32);
/// ```
///
/// Unlike `lind_debug_panic!`, this macro never panics.
#[macro_export]
macro_rules! lind_log {
    // Internal implementation arm — must appear first to prevent the
    // catch-all arm from matching recursive @cat invocations.
    (@cat $category:expr, $($arg:tt)*) => {
        {
            #[cfg(feature = "lind-logging")]
            {
                if $crate::logging::category_enabled($category) {
                    $crate::logging::log(
                        $category,
                        ::core::format_args!($($arg)*),
                        ::core::file!(),
                        ::core::line!(),
                        ::core::module_path!(),
                    );
                }
            }
        }
    };
    // Categorized variants
    (DYLINK, $($arg:tt)*) => {
        $crate::lind_log!(@cat $crate::logging::LogCategory::DYLINK, $($arg)*)
    };
    (THREEI, $($arg:tt)*) => {
        $crate::lind_log!(@cat $crate::logging::LogCategory::THREEI, $($arg)*)
    };
    (Default, $($arg:tt)*) => {
        $crate::lind_log!(@cat $crate::logging::LogCategory::Default, $($arg)*)
    };
    // Default: Default category — must be last
    ($($arg:tt)*) => {
        $crate::lind_log!(@cat $crate::logging::LogCategory::Default, $($arg)*)
    };
}

/// Soft panic for unexpected but potentially survivable conditions.
///
/// Does **not** accept a category — it always fires regardless of the active
/// [`LogCategorySet`].  Use [`lind_log!`] for category-filtered diagnostics.
///
/// Expands to nothing when the `lind-logging` Cargo feature is disabled.
/// Formatting arguments are **not evaluated** in that case.
///
/// # Usage
///
/// ```rust
/// use sysdefs::lind_debug_panic;
///
/// lind_debug_panic!("cage {} not found", 3u64);
/// ```
///
/// # Important
///
/// This macro **may return normally** in `LogOnly` or `NoAction` mode and
/// when `lind-logging` is disabled.  Every call site **must** supply explicit
/// fallback control flow:
///
/// ```rust
/// use sysdefs::lind_debug_panic;
///
/// fn find_cage(id: u64) -> Option<u64> { None }
///
/// let cage = match find_cage(3) {
///     Some(c) => c,
///     None => {
///         lind_debug_panic!("cage {} not found", 3u64);
///         return; // always needed — macro may return in non-panic modes
///     }
/// };
/// ```
#[macro_export]
macro_rules! lind_debug_panic {
    ($($arg:tt)*) => {
        {
            #[cfg(feature = "lind-logging")]
            {
                $crate::logging::debug_panic(
                    ::core::format_args!($($arg)*),
                    ::core::file!(),
                    ::core::line!(),
                    ::core::module_path!(),
                );
            }
        }
    };
}
