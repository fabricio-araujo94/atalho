//! Fuzzy search matching and ranking algorithms for Atalho.
//!
//! Provides a high-performance, fzf-inspired fuzzy matching engine that evaluates
//! queries against [`core::AppEntry`] candidates and produces ranked [`core::ScoredMatch`] results.

use core::{AppEntry, Matcher, ScoredMatch};
use rayon::prelude::*;
use std::cell::RefCell;

// ============================================================================
// Scoring Constants
// ============================================================================

/// Bonus awarded when a matched character directly follows the previously matched character.
/// Rewards tight, contiguous clusters (e.g. "chr" in "Chrome").
pub const BONUS_CONSECUTIVE: i64 = 16;

/// Bonus awarded when a matched character occurs at a word boundary
/// (immediately after '/', '_', '-', '.', whitespace, or a camelCase transition).
pub const BONUS_WORD_BOUNDARY: i64 = 8;

/// Bonus awarded when a match occurs at the first character of the candidate name.
pub const BONUS_START_OF_STRING: i64 = 4;

/// Bonus awarded when the case in candidate matches the query character exactly.
pub const BONUS_CASE_EXACT: i64 = 2;

/// Penalty subtracted for each unmatched character gap between matched query characters.
pub const PENALTY_UNMATCHED_GAP: i64 = 1;

/// Penalty subtracted for each query character not matched (when matching partial patterns).
pub const PENALTY_UNMATCHED_QUERY: i64 = 1;

// ============================================================================
// Thread-Local Scratch Buffers for Zero-Allocation Hot Inner Loop
// ============================================================================

struct MatchScratch {
    char_indices: Vec<usize>,
    chars_target: Vec<char>,
    chars_query: Vec<char>,
    chars_orig: Vec<char>,
    match_positions: Vec<usize>,
}

impl MatchScratch {
    const fn new() -> Self {
        Self {
            char_indices: Vec::new(),
            chars_target: Vec::new(),
            chars_query: Vec::new(),
            chars_orig: Vec::new(),
            match_positions: Vec::new(),
        }
    }

    fn clear(&mut self) {
        self.char_indices.clear();
        self.chars_target.clear();
        self.chars_query.clear();
        self.chars_orig.clear();
        self.match_positions.clear();
    }
}

thread_local! {
    static SCRATCH: RefCell<MatchScratch> = const { RefCell::new(MatchScratch::new()) };
}

// ============================================================================
// Fuzzy Matcher Implementation
// ============================================================================

/// High-performance, zero-allocation fuzzy matcher implementing [`core::Matcher`].
#[derive(Debug, Default, Clone, Copy)]
pub struct FuzzyMatcher;

impl FuzzyMatcher {
    /// Creates a new [`FuzzyMatcher`].
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Evaluates whether candidate characters match a sequential query and calculates the score.
    ///
    /// Returns `Some((score, match_positions))` if all query characters were sequentially found,
    /// or `None` if the candidate does not contain the query sequence.
    #[allow(clippy::cast_possible_wrap)]
    #[must_use]
    pub fn score_candidate(
        query: &str,
        candidate_name: &str,
        normalized_name: &str,
    ) -> Option<(i64, Vec<usize>)> {
        if query.is_empty() {
            return Some((0, Vec::new()));
        }

        SCRATCH.with(|cell| {
            let mut scratch = cell.borrow_mut();
            scratch.clear();

            scratch.chars_query.extend(query.chars());
            scratch.chars_target.extend(normalized_name.chars());

            let query_len = scratch.chars_query.len();
            let target_len = scratch.chars_target.len();

            if query_len > target_len {
                return None;
            }

            // O(n * m) sequential scan finding earliest matching positions
            let mut target_idx = 0;
            for q_idx in 0..query_len {
                let q_char = scratch.chars_query[q_idx];
                let mut found = false;
                while target_idx < target_len {
                    let t_char = scratch.chars_target[target_idx];
                    if t_char == q_char {
                        scratch.char_indices.push(target_idx);
                        target_idx += 1;
                        found = true;
                        break;
                    }
                    target_idx += 1;
                }
                if !found {
                    return None;
                }
            }

            scratch.chars_orig.extend(candidate_name.chars());
            let orig_len = scratch.chars_orig.len();
            let mut score: i64 = 0;
            let mut prev_matched_idx: Option<usize> = None;
            let num_matches = scratch.char_indices.len();

            for pos_in_query in 0..num_matches {
                let idx = scratch.char_indices[pos_in_query];
                // 1. Consecutive match bonus
                if let Some(prev) = prev_matched_idx {
                    if idx == prev + 1 {
                        score += BONUS_CONSECUTIVE;
                    } else {
                        // Gap penalty for unmatched characters in candidate between matches
                        let gap = (idx - prev - 1) as i64;
                        score -= gap * PENALTY_UNMATCHED_GAP;
                    }
                } else {
                    // Leading gap penalty before first match
                    let leading_gap = idx as i64;
                    score -= leading_gap * PENALTY_UNMATCHED_GAP;
                }

                // 2. Start of string bonus
                if idx == 0 {
                    score += BONUS_START_OF_STRING;
                }

                // 3. Word boundary bonus
                if idx > 0 && is_word_boundary(&scratch.chars_orig, idx) {
                    score += BONUS_WORD_BOUNDARY;
                }

                // 4. Exact case match bonus
                if idx < orig_len && pos_in_query < query_len {
                    let orig_char = scratch.chars_orig[idx];
                    let q_raw_char = query.chars().nth(pos_in_query).unwrap_or(orig_char);
                    if orig_char == q_raw_char {
                        score += BONUS_CASE_EXACT;
                    }
                }

                prev_matched_idx = Some(idx);
                scratch.match_positions.push(idx);
            }

            // Penalty for trailing unmatched characters in candidate
            if let Some(last_idx) = prev_matched_idx {
                let trailing_gap = (target_len - last_idx - 1) as i64;
                score -= trailing_gap * PENALTY_UNMATCHED_GAP;
            }

            let positions = scratch.match_positions.clone();
            Some((score, positions))
        })
    }
}

/// Checks if character at `idx` in `chars` is located on a word boundary.
///
/// Boundaries occur after: `/`, `_`, `-`, `.`, whitespace, or during a `camelCase` transition.
#[must_use]
pub const fn is_word_boundary(chars: &[char], idx: usize) -> bool {
    if idx == 0 {
        return true;
    }

    let prev = chars[idx - 1];
    let curr = chars[idx];

    // Delimiter boundary
    if prev == '/'
        || prev == '\\'
        || prev == '_'
        || prev == '-'
        || prev == '.'
        || prev.is_whitespace()
    {
        return true;
    }

    // camelCase transition: lower followed by upper
    if prev.is_lowercase() && curr.is_uppercase() {
        return true;
    }

    false
}

impl Matcher for FuzzyMatcher {
    #[allow(clippy::needless_lifetimes)]
    fn search<'a>(&self, query: &str, entries: &'a [AppEntry]) -> Vec<ScoredMatch> {
        let trimmed_query = query.trim();

        // Architectural Directive 3: Empty query -> return all entries with score 0 (preserving original order)
        if trimmed_query.is_empty() {
            return entries
                .iter()
                .map(|e| ScoredMatch::new(e.clone(), 0, Vec::new()))
                .collect();
        }

        let normalized_query = core::normalize_name(trimmed_query);

        let mut matches = Vec::new();
        for entry in entries {
            if let Some((score, match_positions)) =
                Self::score_candidate(&normalized_query, &entry.name, &entry.normalized_name)
            {
                matches.push(ScoredMatch::new(entry.clone(), score, match_positions));
            }
        }

        // Sort descending by score; preserve relative stability on tie
        matches.sort_by_key(|a| std::cmp::Reverse(a.score));
        matches
    }
}

// ============================================================================
// Parallel Search Implementation (Rayon)
// ============================================================================

/// Executes fuzzy search in parallel across thread pool partitioning entries with Rayon.
///
/// # Arguments
/// * `entries` - Slice of application entries to search.
/// * `query` - Search query string.
/// * `num_threads` - Number of worker threads to use in the pool.
#[must_use]
pub fn parallel_search(entries: &[AppEntry], query: &str, num_threads: usize) -> Vec<ScoredMatch> {
    let trimmed_query = query.trim();
    if trimmed_query.is_empty() {
        return entries
            .iter()
            .map(|e| ScoredMatch::new(e.clone(), 0, Vec::new()))
            .collect();
    }

    let normalized_query = core::normalize_name(trimmed_query);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads.max(1))
        .build();

    let search_fn = || {
        let mut results: Vec<ScoredMatch> = entries
            .par_iter()
            .filter_map(|entry| {
                FuzzyMatcher::score_candidate(
                    &normalized_query,
                    &entry.name,
                    &entry.normalized_name,
                )
                .map(|(score, positions)| ScoredMatch::new(entry.clone(), score, positions))
            })
            .collect();

        results.par_sort_by(|a, b| b.score.cmp(&a.score));
        results
    };

    pool.map_or_else(|_| search_fn(), |p| p.install(search_fn))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::AppSource;

    fn make_app(name: &str) -> AppEntry {
        AppEntry::new(name, format!("C:\\bin\\{name}.exe"), AppSource::StartMenu)
    }

    #[test]
    fn test_empty_query_returns_all_entries_in_order_with_zero_score() {
        let entries = vec![make_app("Alpha"), make_app("Beta"), make_app("Gamma")];
        let matcher = FuzzyMatcher::new();
        let results = matcher.search("", &entries);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].entry.name, "Alpha");
        assert_eq!(results[0].score, 0);
        assert_eq!(results[0].match_positions, [] as [usize; 0]);
        assert_eq!(results[1].entry.name, "Beta");
        assert_eq!(results[2].entry.name, "Gamma");
    }

    #[test]
    fn test_exact_match_scores_higher_than_prefix_or_subsequence() {
        let entries = vec![
            make_app("Google Chrome Canary"),
            make_app("Google Chrome"),
            make_app("Chrome"),
        ];
        let matcher = FuzzyMatcher::new();
        let results = matcher.search("Chrome", &entries);

        assert_ne!(results.len(), 0);
        assert_eq!(results[0].entry.name, "Chrome");
    }

    #[test]
    fn test_camel_case_word_boundary_bonus() {
        let chars_pascal: Vec<char> = "VisualStudioCode".chars().collect();
        assert!(is_word_boundary(&chars_pascal, 0)); // 'V'
        assert!(is_word_boundary(&chars_pascal, 6)); // 'S'
        assert!(is_word_boundary(&chars_pascal, 12)); // 'C'
        assert!(!is_word_boundary(&chars_pascal, 1)); // 'i'
    }

    #[test]
    fn test_delimiter_word_boundary_bonus() {
        let chars_delim: Vec<char> = "vlc-media_player.app/bin".chars().collect();
        assert!(is_word_boundary(&chars_delim, 4)); // 'm' after '-'
        assert!(is_word_boundary(&chars_delim, 10)); // 'p' after '_'
        assert!(is_word_boundary(&chars_delim, 17)); // 'a' after '.'
        assert!(is_word_boundary(&chars_delim, 21)); // 'b' after '/'
    }

    #[test]
    fn test_no_match_returns_empty() {
        let entries = vec![make_app("Calculator"), make_app("Notepad")];
        let matcher = FuzzyMatcher::new();
        let results = matcher.search("xyz", &entries);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_unicode_handling() {
        let entries = vec![make_app("Configurações"), make_app("Área de Trabalho")];
        let matcher = FuzzyMatcher::new();
        let results = matcher.search("config", &entries);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.name, "Configurações");
    }

    #[test]
    fn test_parallel_search_matches_sequential_output() {
        let entries = vec![
            make_app("Google Chrome"),
            make_app("Chromium"),
            make_app("Notepad"),
            make_app("Firefox"),
        ];
        let matcher = FuzzyMatcher::new();
        let seq_results = matcher.search("chr", &entries);
        let par_results = parallel_search(&entries, "chr", 2);

        assert_eq!(seq_results.len(), par_results.len());
        for (s, p) in seq_results.iter().zip(par_results.iter()) {
            assert_eq!(s.entry.name, p.entry.name);
            assert_eq!(s.score, p.score);
        }
    }
}
