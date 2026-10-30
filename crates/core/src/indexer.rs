//! Application indexing contract and source categorization.

use crate::error::Result;
use crate::models::AppEntry;

/// Contract for scanning application sources and discovering launchable desktop applications.
#[async_trait::async_trait]
pub trait Indexer: Send + Sync {
    /// Asynchronously scans application sources and returns discovered application entries.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::IndexingFailed`] if scanning fails.
    async fn scan(&self) -> Result<Vec<AppEntry>>;
}
