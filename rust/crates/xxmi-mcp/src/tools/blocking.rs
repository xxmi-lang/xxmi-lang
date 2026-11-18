//! Runs a synchronous tool on a blocking thread, with a bounded queue in front of the pool.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::SyncTool;
use super::shared::{Args, ToolContext, ToolError, ToolResult};

/// Tool calls park on blocking threads; a small pool keeps their memory bounded.
pub const MAX_BLOCKING_THREADS: usize = 8;

/// Calls allowed to wait for a thread; past that, new ones are turned away at once.
const MAX_QUEUED: usize = 16;

pub struct BlockingGate {
    permits: Arc<Semaphore>,
}

impl BlockingGate {
    pub fn new(capacity: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(capacity)),
        }
    }

    pub fn admit(&self) -> Result<OwnedSemaphorePermit, ToolError> {
        self.permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| ToolError::busy())
    }
}

impl Default for BlockingGate {
    fn default() -> Self {
        Self::new(MAX_BLOCKING_THREADS + MAX_QUEUED)
    }
}

/// Tells a queued task that nobody waits for it any more: the future was dropped by a timeout.
struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// The core's documents are not `Send`, so a tool runs start to finish on one blocking thread.
/// The permit travels into the task and is released only when the work ends, even after a
/// timeout has dropped the caller; a task still queued by then does not start.
pub async fn run_blocking(ctx: Arc<ToolContext>, run: SyncTool, args: Args) -> ToolResult {
    let permit = ctx.blocking.admit()?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let _on_drop = CancelOnDrop(cancelled.clone());

    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if cancelled.load(Ordering::Relaxed) {
            return Err(ToolError::new("cancelled: the call had already timed out"));
        }
        run(&ctx, &args)
    })
    .await
    .map_err(|e| ToolError::internal(format!("tool task failed: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_gate_turns_calls_away_until_a_permit_frees() {
        let gate = BlockingGate::new(1);

        let held = gate.admit().unwrap();
        let busy = gate.admit().unwrap_err();
        drop(held);

        assert!(busy.busy);
        assert!(gate.admit().is_ok());
    }
}
