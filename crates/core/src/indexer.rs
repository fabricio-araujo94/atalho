//! Application indexing contract and source categorization.

use crate::error::Result;
use crate::models::AppEntry;
use tokio_util::sync::CancellationToken;

/// Contract for scanning application sources and discovering launchable desktop applications.
#[async_trait::async_trait]
pub trait Indexer: Send + Sync {
    /// Asynchronously scans application sources and returns discovered application entries.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::IndexingFailed`] if scanning fails.
    async fn scan(&self) -> Result<Vec<AppEntry>>;

    /// Scans with cooperative cancellation.
    ///
    /// Implementations doing long-running work should override this method and
    /// check `token.is_cancelled()` periodically, returning early when cancelled.
    /// The default preserves compatibility with existing indexers, but can only
    /// observe cancellation before and after `scan`.
    async fn scan_with_cancellation(
        &self,
        token: CancellationToken,
    ) -> Result<Option<Vec<AppEntry>>> {
        if token.is_cancelled() {
            return Ok(None);
        }
        let entries = self.scan().await?;
        if token.is_cancelled() {
            Ok(None)
        } else {
            Ok(Some(entries))
        }
    }
}
