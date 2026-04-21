//! Progress reporting for long-running backend operations.

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// A single progress event.
#[derive(Debug, Clone, PartialEq)]
pub enum ProgressEvent {
    /// Operation started with optional known total size (bytes or items).
    Start {
        /// Known total size in bytes or items; `None` if unknown.
        total: Option<u64>,
    },
    /// Progress update: current bytes/items completed and instantaneous rate.
    Update {
        /// Current bytes/items completed so far.
        current: u64,
        /// Instantaneous rate (items or bytes per second), if computable.
        rate: Option<f64>,
    },
    /// Operation completed successfully.
    Complete,
    /// Operation failed with a reason.
    Error(String),
}

/// Sink that receives progress events during long operations.
#[async_trait]
pub trait ProgressSink: Send + Sync {
    /// Emit a single progress event. Implementations should be non-blocking.
    async fn emit(&self, event: ProgressEvent);
}

/// A no-op sink for call sites that don't want progress.
pub struct NoopSink;

#[async_trait]
impl ProgressSink for NoopSink {
    async fn emit(&self, _event: ProgressEvent) {}
}

/// A sink that collects all events in memory; useful for tests.
#[derive(Default)]
pub struct MemorySink {
    events: Arc<Mutex<Vec<ProgressEvent>>>,
}

impl MemorySink {
    /// Construct an empty `MemorySink`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all events captured so far.
    pub fn events(&self) -> Vec<ProgressEvent> {
        self.events.lock().unwrap().clone()
    }
}

#[async_trait]
impl ProgressSink for MemorySink {
    async fn emit(&self, event: ProgressEvent) {
        self.events.lock().unwrap().push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_sink_swallows() {
        let s = NoopSink;
        s.emit(ProgressEvent::Start { total: Some(10) }).await;
        s.emit(ProgressEvent::Complete).await;
    }

    #[tokio::test]
    async fn memory_sink_collects_events() {
        let s = MemorySink::new();
        s.emit(ProgressEvent::Start { total: Some(100) }).await;
        s.emit(ProgressEvent::Update {
            current: 50,
            rate: Some(42.0),
        })
        .await;
        s.emit(ProgressEvent::Complete).await;
        assert_eq!(s.events().len(), 3);
        assert_eq!(s.events()[0], ProgressEvent::Start { total: Some(100) });
    }
}
