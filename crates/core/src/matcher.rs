//! Fuzzy matching contracts and scored candidate models.

use crate::models::AppEntry;

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
}
