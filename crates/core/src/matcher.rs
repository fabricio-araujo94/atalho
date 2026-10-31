//! Fuzzy matching contracts and scored candidate models.

use crate::models::AppEntry;
use tokio_util::sync::CancellationToken;

/// A scored search match candidate containing the matched application entry,
/// its calculated relevance score, and matching character indices in the display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredMatch {
    /// The matched application entry.
    pub entry: AppEntry,
    /// Relevance score (higher represents higher confidence/quality).
    pub score: i64,
    /// Character indices in candidate name for visual highlighting.
    pub match_positions: Vec<usize>,
}

impl ScoredMatch {
    /// Creates a new [`ScoredMatch`].
    #[must_use]
    pub const fn new(entry: AppEntry, score: i64, match_positions: Vec<usize>) -> Self {
        Self {
            entry,
            score,
            match_positions,
        }
    }
}

/// Synchronous fuzzy matcher contract ranking candidates against a query string.
pub trait Matcher: Send + Sync {
    /// Evaluates the query against a slice of candidate application entries and returns
    /// ranked matches sorted by score in descending order.
    #[allow(clippy::needless_lifetimes)]
    fn search<'a>(&self, query: &str, entries: &'a [AppEntry]) -> Vec<ScoredMatch>;

    /// Performs matching with cooperative cancellation.
    ///
    /// Implementations with long-running matching work should check the token
    /// periodically. The default keeps existing implementations compatible and
    /// observes cancellation before and after the synchronous search.
    fn search_with_cancellation(
        &self,
        query: &str,
        entries: &[AppEntry],
        token: &CancellationToken,
    ) -> Option<Vec<ScoredMatch>> {
        if token.is_cancelled() {
            return None;
        }
        let results = self.search(query, entries);
        (!token.is_cancelled()).then_some(results)
    }
}

/// Request parameters for searching applications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    /// Search query string.
    pub query: String,
    /// Maximum number of results to return.
    pub max_results: usize,
}

impl SearchRequest {
    /// Creates a new [`SearchRequest`].
    #[must_use]
    pub fn new(query: impl Into<String>, max_results: usize) -> Self {
        Self {
            query: query.into(),
            max_results,
        }
    }
}

/// Response containing search results, total match count, and execution duration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResponse {
    /// Ranked search results matching the query (limited to `max_results`).
    pub results: Vec<ScoredMatch>,
    /// Total number of matching entries found before truncation.
    pub total: usize,
    /// Time taken to execute the search in microseconds.
    pub elapsed_us: u64,
}

impl SearchResponse {
    /// Creates a new [`SearchResponse`].
    #[must_use]
    pub const fn new(results: Vec<ScoredMatch>, total: usize, elapsed_us: u64) -> Self {
        Self {
            results,
            total,
            elapsed_us,
        }
    }
}
