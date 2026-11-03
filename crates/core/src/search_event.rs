//! Search lifecycle events shared between the service and presentation layers.

use crate::{AppError, SearchResponse};

/// Search lifecycle notifications sent to UI subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEvent {
    /// A query has been submitted and is waiting to complete.
    Loading,
    /// A query completed successfully.
    Results(SearchResponse),
    /// A search or indexing operation failed.
    Error(AppError),
}
