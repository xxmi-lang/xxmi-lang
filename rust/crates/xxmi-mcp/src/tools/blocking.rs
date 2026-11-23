//! Runs a synchronous tool on a blocking thread, with a bounded queue in front of the pool.

use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::SyncTool;
use super::shared::{Args, ToolContext, ToolError, ToolResult};
use crate::cancel::{CancelFlag, cancelled};

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

/// Tells the task that nobody waits for it any more: the future was dropped by a timeout.
struct CancelOnDrop(CancelFlag);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub fn cancelled_error() -> ToolError {
    ToolError::new("cancelled: the call had already timed out")
}

/// Stops a tool at a natural loop point once its call has timed out.
pub fn checkpoint() -> Result<(), ToolError> {
    if cancelled() {
        return Err(cancelled_error());
    }
    Ok(())
}

pub async fn run_blocking(ctx: Arc<ToolContext>, run: SyncTool, args: Args) -> ToolResult {
    let permit = ctx.blocking.admit()?;
    run_job(permit, move || run(&ctx, &args)).await
}

/// The core's documents are not `Send`, so a tool runs start to finish on one blocking thread.
/// The permit travels into the task and is released only when the work ends, even after a timeout
/// has dropped the caller; a task still queued by then does not start,
/// and a running one sees the timeout at its `checkpoint`s.
pub async fn run_job(
    permit: OwnedSemaphorePermit,
    job: impl FnOnce() -> ToolResult + Send + 'static,
) -> ToolResult {
    let flag = CancelFlag::default();
    let _on_drop = CancelOnDrop(flag.clone());

    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _installed = flag.install();
        checkpoint()?;
        job()
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
