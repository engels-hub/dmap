//! Opt-in diagnostics for the fullscreen memory investigation.
//! Enable with the `DMAP_DEBUG` environment variable (any value).

use std::sync::OnceLock;
use std::time::{Duration, Instant};

static START: OnceLock<Instant> = OnceLock::new();

pub fn enabled() -> bool {
    std::env::var_os("DMAP_DEBUG").is_some()
}

/// When to stop the app by itself, from `DMAP_EXIT_AFTER` in whole seconds.
///
/// The profiler writes its report only when the app exits through the end of
/// `main`. A window that flickers cannot be closed by hand, so the app needs
/// to close itself instead.
pub fn exit_deadline() -> Option<Instant> {
    let seconds: u64 = std::env::var("DMAP_EXIT_AFTER").ok()?.parse().ok()?;
    Some(Instant::now() + Duration::from_secs(seconds))
}

pub fn log(args: std::fmt::Arguments) {
    if enabled() {
        let start = *START.get_or_init(Instant::now);
        eprintln!(
            "[{:>9.3}ms] {}",
            start.elapsed().as_secs_f64() * 1000.0,
            args
        );
    }
}

macro_rules! dbg_log {
    ($($arg:tt)*) => { $crate::diag::log(format_args!($($arg)*)) };
}
pub(crate) use dbg_log;
