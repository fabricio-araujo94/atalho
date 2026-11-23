//! Concurrent composite application indexer.

use atalho_core::{AppEntry, AppSource, Indexer, Result};
use std::collections::HashSet;
use std::path::Path;
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

    /// Creates a composite indexer containing only the requested application sources.
    #[must_use]
    pub fn from_sources(sources: &[AppSource]) -> Self {
        let mut indexers: Vec<Arc<dyn Indexer>> = Vec::new();
        let mut start_menu = false;
        let mut desktop = false;
        let mut path = false;
        for source in sources {
            match source {
                AppSource::StartMenu if !start_menu => {
                    start_menu = true;
                    indexers.push(Arc::new(StartMenuIndexer::new()));
                }
                AppSource::Desktop if !desktop => {
                    desktop = true;
                    indexers.push(Arc::new(DesktopIndexer::new()));
                }
                AppSource::Path if !path => {
                    path = true;
                    indexers.push(Arc::new(PathIndexer::new()));
                }
                AppSource::StartMenu
                | AppSource::Desktop
                | AppSource::Path
                | AppSource::UserPinned => {}
            }
        }
        Self::new(indexers)
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
        let mut seen: HashSet<(&str, &Path)> = HashSet::with_capacity(all_entries.len());
        let mut keep_entry = Vec::with_capacity(all_entries.len());

        for entry in &all_entries {
            if token.is_cancelled() {
                return Ok(None);
            }
            keep_entry.push(seen.insert((
                entry.normalized_name.as_str(),
                entry.executable_path.as_path(),
            )));
        }
        drop(seen);

        let mut deduped_entries = Vec::with_capacity(all_entries.len());
        for (entry, keep) in all_entries.into_iter().zip(keep_entry) {
            if token.is_cancelled() {
                return Ok(None);
            }
            if keep {
                deduped_entries.push(entry);
            }
        }

        Ok(Some(deduped_entries))
    }
}
