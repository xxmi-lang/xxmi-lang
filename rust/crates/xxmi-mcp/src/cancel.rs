//! Lets a tool running on a blocking thread notice that its call has timed out.
//! The flag is per thread because a tool runs start to finish on one thread,
//! so the loops deep inside it can check without every signature carrying a token.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

thread_local! {
    static CURRENT: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// Set when nobody waits for the call any more: the future was dropped by a timeout.
#[derive(Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Makes `cancelled()` on this thread report this flag until the guard drops.
    pub fn install(&self) -> Installed {
        CURRENT.with(|current| *current.borrow_mut() = Some(self.0.clone()));
        Installed
    }
}

/// Blocking threads are reused, so a finished tool must not leave its flag behind.
pub struct Installed;

impl Drop for Installed {
    fn drop(&mut self) {
        CURRENT.with(|current| *current.borrow_mut() = None);
    }
}

/// True when the call running on this thread has been given up on; false outside a tool call.
pub fn cancelled() -> bool {
    CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    })
}
