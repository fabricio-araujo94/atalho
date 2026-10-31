//! Concurrent composite application indexer.

use core::{AppEntry, AppSource, Indexer, Result};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::desktop::DesktopIndexer;
use crate::path::PathIndexer;
use crate::start_menu::StartMenuIndexer;

/// Composite indexer that executes multiple indexers concurrently and deduplicates results.
#[derive(Clone)]
pub struct CompositeIndexer {
    indexers: Vec<Arc<dyn Indexer>>,
}

impl Default for CompositeIndexer {
    fn default() -> Self {
        Self::new(vec![
            Arc::new(StartMenuIndexer::new()),
            Arc::new(DesktopIndexer::new()),
            Arc::new(PathIndexer::new()),
        ])
    }
}

impl std::fmt::Debug for CompositeIndexer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeIndexer")
            .field("indexer_count", &self.indexers.len())
            .finish()
    }
}

impl CompositeIndexer {
    /// Creates a new [`CompositeIndexer`] with the specified collection of child indexers.
    #[must_use]
    pub fn new(indexers: Vec<Arc<dyn Indexer>>) -> Self {
        Self { indexers }
    }

    /// Adds an additional [`Indexer`] to this composite indexer.
    #[must_use]
    pub fn with_indexer(mut self, indexer: impl Indexer + 'static) -> Self {
        self.indexers.push(Arc::new(indexer));
        self
    }

    /// Assigns a ranking priority to application sources for deterministic deduplication.
    const fn source_priority(source: AppSource) -> u8 {
        match source {
            AppSource::UserPinned => 0,
            AppSource::StartMenu => 1,
            AppSource::Desktop => 2,
            AppSource::Path => 3,
        }
    }
}

#[async_trait::async_trait]
impl Indexer for CompositeIndexer {
    async fn scan(&self) -> Result<Vec<AppEntry>> {
        Ok(self
            .scan_with_cancellation(CancellationToken::new())
            .await?
            .unwrap_or_default())
    }

    async fn scan_with_cancellation(
        &self,
        token: CancellationToken,
    ) -> Result<Option<Vec<AppEntry>>> {
        let mut join_set = JoinSet::new();

        for indexer in &self.indexers {
            let idx = Arc::clone(indexer);
            let child_token = token.clone();
            join_set.spawn(async move { idx.scan_with_cancellation(child_token).await });
        }

        let mut all_entries = Vec::new();

        while let Some(res) = join_set.join_next().await {
            if token.is_cancelled() {
                join_set.abort_all();
                return Ok(None);
            }
            match res {
                Ok(Ok(Some(entries))) => {
                    all_entries.extend(entries);
                }
                Ok(Ok(None)) => {
                    join_set.abort_all();
                    return Ok(None);
                }
                Ok(Err(err)) => {
                    warn!("Indexer scan returned error: {err}");
                }
                Err(join_err) => {
                    warn!("Indexer scan task failed to join: {join_err}");
                }
            }
        }

        // Sort by source priority so higher quality entries (StartMenu, Desktop) precede PATH
        all_entries.sort_by_key(|entry| Self::source_priority(entry.source));

        // Deduplicate by (normalized_name, executable_path) as specified in Architectural Directive 4
        let mut seen: HashSet<(String, PathBuf)> = HashSet::new();
        let mut deduped_entries = Vec::with_capacity(all_entries.len());

        for entry in all_entries {
            if token.is_cancelled() {
                return Ok(None);
            }
            let key = (entry.normalized_name.clone(), entry.executable_path.clone());
            if seen.insert(key) {
                deduped_entries.push(entry);
            }
        }

        Ok(Some(deduped_entries))
    }
}
