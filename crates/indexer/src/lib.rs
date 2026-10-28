//! Application and command indexing engine for Atalho.
//!
//! Provides concrete implementations of [`core::Indexer`] to scan installed desktop
//! applications and discovered command actions.

use core::{AppEntry, CommandEntry, Indexer, Result};

/// Default application and command indexer implementation.
#[derive(Debug, Default, Clone)]
pub struct DefaultIndexer {
    /// In-memory cache or preloaded applications.
    preloaded_apps: Vec<AppEntry>,
    /// In-memory cache or preloaded commands.
    preloaded_commands: Vec<CommandEntry>,
}

impl DefaultIndexer {
    /// Creates a new, empty [`DefaultIndexer`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new [`DefaultIndexer`] populated with the provided applications and commands.
    #[must_use]
    pub fn with_entries(apps: Vec<AppEntry>, commands: Vec<CommandEntry>) -> Self {
        Self {
            preloaded_apps: apps,
            preloaded_commands: commands,
        }
    }
}

#[async_trait::async_trait]
impl Indexer for DefaultIndexer {
    async fn index_apps(&self) -> Result<Vec<AppEntry>> {
        Ok(self.preloaded_apps.clone())
    }

    async fn index_commands(&self) -> Result<Vec<CommandEntry>> {
        Ok(self.preloaded_commands.clone())
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
    fn test_indexer_returns_entries() {
        let apps = vec![AppEntry::new("test", "Test App", "/bin/test")];
        let commands = vec![CommandEntry::new("cmd", "Test Cmd", "echo")];
        let indexer = DefaultIndexer::with_entries(apps.clone(), commands.clone());

        let indexed_apps = block_on(indexer.index_apps());
        assert_eq!(indexed_apps.as_ref().map(Vec::len), Ok(1));
        assert_eq!(indexed_apps.unwrap_or_default(), apps);

        let indexed_commands = block_on(indexer.index_commands());
        assert_eq!(indexed_commands.as_ref().map(Vec::len), Ok(1));
        assert_eq!(indexed_commands.unwrap_or_default(), commands);
    }
}
