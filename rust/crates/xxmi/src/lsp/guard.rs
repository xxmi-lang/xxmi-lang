//! Panic containment: a bad input must never end the session, see docs/03-lsp.md "Robustness".

use std::panic::{AssertUnwindSafe, catch_unwind};

/// `None` when `work` panicked; the panic hook has already logged it to stderr.
///
/// The state `work` borrowed may be half updated, which is why callers re-sync what they touched.
pub fn guarded<T>(work: impl FnOnce() -> T) -> Option<T> {
    catch_unwind(AssertUnwindSafe(work)).ok()
}
