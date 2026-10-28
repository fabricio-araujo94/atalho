//! Iced-based graphical user interface for Atalho.
//!
//! Provides presentation state models, message definitions, and view components
//! for the desktop search and launcher interface.

use core::SearchResult;

/// Messages emitted by user interaction within the launcher UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LauncherMessage {
    /// Search query input text was modified.
    QueryChanged(String),
    /// Result item at the specified index was activated or selected.
    ItemSelected(usize),
    /// Keyboard navigation moved the highlight cursor up.
    SelectPrevious,
    /// Keyboard navigation moved the highlight cursor down.
    SelectNext,
    /// Currently highlighted item was submitted for execution.
    Submit,
    /// Launcher window requested to close or dismiss.
    Dismiss,
}

/// State representation for the launcher search window.
#[derive(Debug, Default, Clone)]
pub struct LauncherState {
    /// Active query text in the search input box.
    pub query: String,
    /// Current list of filtered and ranked search results.
    pub results: Vec<SearchResult>,
    /// Index of the currently highlighted result item, if any.
    pub selected_index: Option<usize>,
}

impl LauncherState {
    /// Creates a new, blank [`LauncherState`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates the search query and resets the selected index if needed.
    pub fn update_query(&mut self, query: String) {
        self.query = query;
        if self.results.is_empty() {
            self.selected_index = None;
        } else {
            self.selected_index = Some(0);
        }
    }

    /// Sets the list of matched search results.
    pub fn set_results(&mut self, results: Vec<SearchResult>) {
        self.results = results;
        if self.results.is_empty() {
            self.selected_index = None;
        } else {
            self.selected_index = Some(0);
        }
    }

    /// Navigates highlight to the next search result.
    pub fn select_next(&mut self) {
        if self.results.is_empty() {
            self.selected_index = None;
            return;
        }

        self.selected_index = Some(match self.selected_index {
            Some(idx) if idx + 1 < self.results.len() => idx + 1,
            _ => 0,
        });
    }

    /// Navigates highlight to the previous search result.
    pub fn select_previous(&mut self) {
        if self.results.is_empty() {
            self.selected_index = None;
            return;
        }

        self.selected_index = Some(match self.selected_index {
            Some(0) | None => self.results.len() - 1,
            Some(idx) => idx - 1,
        });
    }

    /// Returns a reference to the currently selected search result item, if any.
    #[must_use]
    pub fn selected_result(&self) -> Option<&SearchResult> {
        self.selected_index.and_then(|idx| self.results.get(idx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{AppEntry, Score};

    #[test]
    fn test_launcher_navigation() {
        let mut state = LauncherState::new();
        assert_eq!(state.selected_result(), None);

        let app1 = AppEntry::new("app1", "First App", "/bin/first");
        let app2 = AppEntry::new("app2", "Second App", "/bin/second");
        let results = vec![
            SearchResult::from_app(app1, Score::new(100)),
            SearchResult::from_app(app2, Score::new(50)),
        ];

        state.set_results(results);
        assert_eq!(state.selected_index, Some(0));

        state.select_next();
        assert_eq!(state.selected_index, Some(1));

        state.select_next();
        assert_eq!(state.selected_index, Some(0));

        state.select_previous();
        assert_eq!(state.selected_index, Some(1));
    }
}
