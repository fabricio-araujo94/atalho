//! Fuzzy search matching and ranking algorithms for Atalho.
//!
//! Provides implementations of [`core::Matcher`] that calculate relevance scores and
//! character highlight indices for candidate applications and commands.

use core::{AppEntry, CommandEntry, Matcher, Result, Score, SearchResult};

/// High-performance fuzzy and prefix matcher.
#[derive(Debug, Default, Clone, Copy)]
pub struct FuzzyMatcher;

impl FuzzyMatcher {
    /// Creates a new [`FuzzyMatcher`].
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Calculates match score and matched character indices for a candidate text against a query.
    #[must_use]
    pub fn score_text(query: &str, target: &str) -> Option<(Score, Vec<usize>)> {
        if query.is_empty() {
            return Some((Score::new(1), Vec::new()));
        }

        let query_lower = query.to_lowercase();
        let target_lower = target.to_lowercase();

        // Exact match receives highest priority
        if target_lower == query_lower {
            let indices: Vec<usize> = (0..target.chars().count()).collect();
            return Some((Score::new(1000), indices));
        }

        // Prefix match receives high priority
        if target_lower.starts_with(&query_lower) {
            let count = query.chars().count();
            let indices: Vec<usize> = (0..count).collect();
            return Some((Score::new(500), indices));
        }

        // Substring match receives medium priority
        if let Some(byte_idx) = target_lower.find(&query_lower) {
            let char_start = target_lower[..byte_idx].chars().count();
            let char_len = query.chars().count();
            let indices: Vec<usize> = (char_start..char_start + char_len).collect();
            return Some((Score::new(250), indices));
        }

        // Fuzzy subsequence match
        let mut indices = Vec::new();
        let mut target_chars = target_lower.char_indices().enumerate();

        for q_char in query_lower.chars() {
            let found = target_chars.by_ref().find(|(_, (_, t_char))| *t_char == q_char);
            match found {
                Some((char_idx, _)) => indices.push(char_idx),
                None => return None,
            }
        }

        Some((Score::new(100), indices))
    }
}

impl Matcher for FuzzyMatcher {
    fn match_apps(&self, query: &str, apps: &[AppEntry]) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();

        for app in apps {
            let title_match = Self::score_text(query, &app.name);
            let keyword_match = app.keywords.iter().find_map(|kw| Self::score_text(query, kw));

            let match_data = match (title_match, keyword_match) {
                (Some((score, indices)), _) => Some((score, indices)),
                (None, Some((score, _))) => Some((Score::new(score.value() / 2), Vec::new())),
                (None, None) => None,
            };

            if let Some((score, indices)) = match_data {
                results.push(SearchResult::from_app(app.clone(), score).with_match_indices(indices));
            }
        }

        results.sort_by(|a, b| b.score.cmp(&a.score));
        Ok(results)
    }

    fn match_commands(&self, query: &str, commands: &[CommandEntry]) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();

        for cmd in commands {
            let title_match = Self::score_text(query, &cmd.title);
            let keyword_match = cmd.keywords.iter().find_map(|kw| Self::score_text(query, kw));

            let match_data = match (title_match, keyword_match) {
                (Some((score, indices)), _) => Some((score, indices)),
                (None, Some((score, _))) => Some((Score::new(score.value() / 2), Vec::new())),
                (None, None) => None,
            };

            if let Some((score, indices)) = match_data {
                results.push(SearchResult::from_command(cmd.clone(), score).with_match_indices(indices));
            }
        }

        results.sort_by(|a, b| b.score.cmp(&a.score));
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_and_prefix_matching() {
        let matcher = FuzzyMatcher::new();
        let apps = vec![
            AppEntry::new("firefox", "Firefox", "/bin/firefox"),
            AppEntry::new("firewall", "Firewall Configuration", "/bin/firewall"),
            AppEntry::new("terminal", "Terminal", "/bin/bash"),
        ];

        let results = matcher.match_apps("Firefox", &apps).unwrap_or_default();
        assert_eq!(results.first().map(|r| r.id.as_str()), Some("firefox"));
        assert_eq!(results.first().map(|r| r.score.value()), Some(1000));

        let prefix_results = matcher.match_apps("Fire", &apps).unwrap_or_default();
        assert_eq!(prefix_results.len(), 2);
    }

    #[test]
    fn test_search_combines_apps_and_commands() {
        let matcher = FuzzyMatcher::new();
        let apps = vec![AppEntry::new("calc", "Calculator", "/bin/calc")];
        let commands = vec![CommandEntry::new("calc-cmd", "Calculate Sum", "calc")];

        let results = matcher.search("Calc", &apps, &commands).unwrap_or_default();
        assert_eq!(results.len(), 2);
    }
}
