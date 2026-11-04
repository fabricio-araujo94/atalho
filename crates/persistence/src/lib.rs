//! State, preferences, and history persistence layer for Atalho.
//!
//! Provides implementations of [`core::Persistence`] for storing application lists,
//! search cache, and launch metrics.

use core::{AppConfig, AppEntry, AppError, Config, History, Persistence, Result};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::SystemTime;

mod disk;
pub use disk::FilePersistence;
#[cfg(any(test, feature = "test-utils"))]
pub use disk::{InMemoryCache, InMemoryHistory};

/// In-memory thread-safe implementation of [`Persistence`].
#[derive(Debug, Default)]
pub struct InMemoryPersistence {
    apps: RwLock<Vec<AppEntry>>,
    apps_updated_at: RwLock<Option<SystemTime>>,
    launch_history: RwLock<Vec<String>>,
    typed_history: RwLock<Vec<uuid::Uuid>>,
    config: RwLock<AppConfig>,
}

#[async_trait::async_trait]
impl History for InMemoryPersistence {
    async fn record(&self, entry_id: uuid::Uuid) -> Result<()> {
        self.typed_history
            .write()
            .map_err(|err| AppError::io_failed(format!("Lock acquisition error: {err}")))?
            .push(entry_id);
        Ok(())
    }

    async fn recent(&self, limit: usize) -> Result<Vec<uuid::Uuid>> {
        let history = self
            .typed_history
            .read()
            .map_err(|err| AppError::io_failed(format!("Lock acquisition error: {err}")))?;
        Ok(history.iter().rev().take(limit).copied().collect())
    }

    async fn frequency(&self) -> Result<HashMap<uuid::Uuid, u32>> {
        let frequency = {
            let history = self
                .typed_history
                .read()
                .map_err(|err| AppError::io_failed(format!("Lock acquisition error: {err}")))?;
            let mut frequency = HashMap::new();
            for id in history.iter() {
                let count = frequency.entry(*id).or_insert(0_u32);
                *count = count.saturating_add(1);
            }
            drop(history);
            frequency
        };
        Ok(frequency)
    }
}

impl InMemoryPersistence {
    /// Creates a new, empty [`InMemoryPersistence`] instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Config for InMemoryPersistence {
    fn load(&self) -> Result<AppConfig> {
        self.config
            .read()
            .map(|config| config.clone())
            .map_err(|error| AppError::io_failed(format!("Config lock acquisition error: {error}")))
    }

    fn save(&self, config: &AppConfig) -> Result<()> {
        *self.config.write().map_err(|error| {
            AppError::io_failed(format!("Config lock acquisition error: {error}"))
        })? = config.clone();
        Ok(())
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
        drop(guard);
        *self
            .apps_updated_at
            .write()
            .map_err(|err| AppError::io_failed(format!("Lock acquisition error: {err}")))? =
            Some(SystemTime::now());
        Ok(())
    }

    async fn load_apps(&self) -> Result<Vec<AppEntry>> {
        let guard = self
            .apps
            .read()
            .map_err(|err| AppError::io_failed(format!("Lock acquisition error: {err}")))?;
        Ok(guard.clone())
    }

    async fn apps_last_updated(&self) -> Result<Option<SystemTime>> {
        self.apps_updated_at
            .read()
            .map(|guard| *guard)
            .map_err(|err| AppError::io_failed(format!("Lock acquisition error: {err}")))
    }

    async fn record_launch(&self, entry_id: &str) -> Result<()> {
        self.launch_history
            .write()
            .map_err(|err| AppError::io_failed(format!("Lock acquisition error: {err}")))?
            .push(entry_id.to_string());
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
        let apps = vec![AppEntry::new(
            "Editor",
            "/bin/editor",
            core::AppSource::Path,
        )];

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
