//! State, preferences, and history persistence layer for Atalho.
//!
//! Provides implementations of [`core::Persistence`] for storing application lists,
//! search cache, and launch metrics.

use std::sync::RwLock;
use core::{AppEntry, AppError, Persistence, Result};

/// In-memory thread-safe implementation of [`Persistence`].
#[derive(Debug, Default)]
pub struct InMemoryPersistence {
    apps: RwLock<Vec<AppEntry>>,
    launch_history: RwLock<Vec<String>>,
}

impl InMemoryPersistence {
    /// Creates a new, empty [`InMemoryPersistence`] instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl Persistence for InMemoryPersistence {
    async fn save_apps(&self, apps: &[AppEntry]) -> Result<()> {
        let mut guard = self
            .apps
            .write()
            .map_err(|err| AppError::io_failed(format!("Lock acquisition error: {err}")))?;
        guard.clear();
        guard.extend_from_slice(apps);
        Ok(())
    }

    async fn load_apps(&self) -> Result<Vec<AppEntry>> {
        let guard = self
            .apps
            .read()
            .map_err(|err| AppError::io_failed(format!("Lock acquisition error: {err}")))?;
        Ok(guard.clone())
    }

    async fn record_launch(&self, entry_id: &str) -> Result<()> {
        let mut guard = self
            .launch_history
            .write()
            .map_err(|err| AppError::io_failed(format!("Lock acquisition error: {err}")))?;
        guard.push(entry_id.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn block_on<F: Future>(future: F) -> F::Output {
        fn noop_raw_waker() -> RawWaker {
            fn no_op(_: *const ()) {}
            fn clone_waker(_: *const ()) -> RawWaker {
                noop_raw_waker()
            }
            let vtable = &RawWakerVTable::new(clone_waker, no_op, no_op, no_op);
            RawWaker::new(std::ptr::null(), vtable)
        }

        let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
        let mut cx = Context::from_waker(&waker);
        let mut pinned = pin!(future);
        match pinned.as_mut().poll(&mut cx) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("Future remained pending"),
        }
    }

    #[test]
    fn test_save_and_load_apps() {
        let persistence = InMemoryPersistence::new();
        let apps = vec![AppEntry::new("editor", "Editor", "/bin/editor")];

        assert!(block_on(persistence.save_apps(&apps)).is_ok());
        let loaded = block_on(persistence.load_apps()).unwrap_or_default();
        assert_eq!(loaded, apps);
    }

    #[test]
    fn test_record_launch() {
        let persistence = InMemoryPersistence::new();
        assert!(block_on(persistence.record_launch("editor")).is_ok());
    }
}
