//! Opt-in timing for the edit path: with `XXMI_TRACE_TIMING=1` each span prints its duration to
//! stderr. Off, a span costs one relaxed load.

use std::sync::OnceLock;
use std::time::Instant;

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("XXMI_TRACE_TIMING").is_some_and(|v| v == "1"))
}

/// Runs `work` and, when tracing, prints `xxmi-trace <label> <milliseconds>`.
pub fn span<T>(label: &str, work: impl FnOnce() -> T) -> T {
    if !enabled() {
        return work();
    }

    let started = Instant::now();
    let result = work();
    eprintln!(
        "xxmi-trace {label} {:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    result
}
