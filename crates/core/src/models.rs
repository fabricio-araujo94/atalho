//! Core domain models representing applications, commands, search hits, and relevance scoring.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Numerical relevance score assigned to search matches.
///
/// Higher scores represent higher confidence or better match quality. Scores are
/// completely ordered to permit deterministic ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Score(pub u32);

impl Score {
    /// Zero relevance, indicating no match.
    pub const ZERO: Self = Self(0);

    /// Maximum possible score, indicating an exact or perfect match.
    pub const MAX: Self = Self(u32::MAX);

    /// Creates a new [`Score`] with the specified value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the inner numerical score value.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Origin source from which an application entry was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AppSource {
    /// Discovered from Windows Start Menu shortcuts.
    StartMenu,
    /// Discovered from system PATH directories.
    #[serde(rename = "PATH")]
    Path,
    /// Discovered from user or public desktop shortcuts.
    Desktop,
    /// Manually pinned by the user.
    UserPinned,
}

/// Produces a normalized lookup string for an application name by converting to lowercase
/// and removing ASCII punctuation characters.
#[must_use]
pub fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_ascii_punctuation())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Represents an indexed desktop application available for launching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppEntry {
    /// Unique identifier for this application entry.
    pub id: Uuid,
    /// Human-readable display name of the application.
    pub name: String,
    /// Normalized name used for deduplication and matching (lowercase, no punctuation).
    pub normalized_name: String,
    /// Absolute or resolvable path to the application executable or desktop file.
    pub executable_path: PathBuf,
    /// Optional command-line arguments to pass when executing this application.
    pub arguments: Option<String>,
    /// Optional path to the application icon image file.
    pub icon_path: Option<PathBuf>,
    /// Origin source where the application was discovered.
    pub source: AppSource,
    /// Optional human-readable description of the application.
    pub description: Option<String>,
    /// Optional search keywords or aliases used to match this application.
    pub keywords: Vec<String>,
}

impl AppEntry {
    /// Creates a new [`AppEntry`] with required fields and defaults for optional metadata.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        executable_path: impl Into<PathBuf>,
        source: AppSource,
    ) -> Self {
        let name_str = name.into();
        let normalized = normalize_name(&name_str);
        Self {
            id: Uuid::new_v4(),
            name: name_str,
            normalized_name: normalized,
            executable_path: executable_path.into(),
            arguments: None,
            icon_path: None,
            source,
            description: None,
            keywords: Vec::new(),
        }
    }

    /// Sets the unique ID for this entry.
    #[must_use]
    pub const fn with_id(mut self, id: Uuid) -> Self {
        self.id = id;
        self
    }

    /// Sets optional command-line arguments for launching this application.
    #[must_use]
    pub fn with_arguments(mut self, arguments: impl Into<String>) -> Self {
        self.arguments = Some(arguments.into());
        self
    }

    /// Sets the optional icon path for the application.
    #[must_use]
    pub fn with_icon(mut self, icon_path: impl Into<PathBuf>) -> Self {
        self.icon_path = Some(icon_path.into());
        self
    }

    /// Sets the optional human-readable description for the application.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds search keywords or aliases to the application.
    #[must_use]
    pub fn with_keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.keywords.extend(keywords.into_iter().map(Into::into));
        self
    }

    /// Returns a reference to the executable path.
    #[must_use]
    pub fn exec_path(&self) -> &Path {
        &self.executable_path
    }
}

/// Represents an executable system or user-defined command action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEntry {
    /// Unique identifier for the command action.
    pub id: String,
    /// Human-readable title displayed in search listings.
    pub title: String,
    /// Executable binary, script, or system shell command string.
    pub command: String,
    /// Arguments passed to the command upon invocation.
    pub args: Vec<String>,
    /// Optional description explaining what the command performs.
    pub description: Option<String>,
    /// Optional search keywords or tags.
    pub keywords: Vec<String>,
}

impl CommandEntry {
    /// Creates a new [`CommandEntry`] with required fields and empty defaults for optional metadata.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            command: command.into(),
            args: Vec::new(),
            description: None,
            keywords: Vec::new(),
        }
    }

    /// Adds arguments to the command invocation.
    #[must_use]
    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Sets the optional description for the command.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds search keywords or aliases to the command.
    #[must_use]
    pub fn with_keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.keywords.extend(keywords.into_iter().map(Into::into));
        self
    }
}

/// Enumeration of underlying target items encapsulated by a search match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchItem {
    /// An indexed desktop application.
    App(AppEntry),
    /// An executable command action.
    Command(CommandEntry),
}

/// Represents an individual ranked hit produced by matching a search query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    /// Unique identifier of the matched item.
    pub id: String,
    /// Primary title to display for this result item.
    pub title: String,
    /// Optional secondary text or description.
    pub description: Option<String>,
    /// Relevance score assigned by the matcher.
    pub score: Score,
    /// The underlying matched domain item (application or command).
    pub item: SearchItem,
    /// Character indices within the primary title that matched the query, for UI highlighting.
    pub match_indices: Vec<usize>,
}

impl SearchResult {
    /// Constructs a [`SearchResult`] wrapping an [`AppEntry`].
    #[must_use]
    pub fn from_app(app: AppEntry, score: Score) -> Self {
        Self {
            id: app.id.to_string(),
            title: app.name.clone(),
            description: app.description.clone(),
            score,
            item: SearchItem::App(app),
            match_indices: Vec::new(),
        }
    }

    /// Constructs a [`SearchResult`] wrapping a [`CommandEntry`].
    #[must_use]
    pub fn from_command(command: CommandEntry, score: Score) -> Self {
        Self {
            id: command.id.clone(),
            title: command.title.clone(),
            description: command.description.clone(),
            score,
            item: SearchItem::Command(command),
            match_indices: Vec::new(),
        }
    }

    /// Attaches matched character indices for UI highlighting.
    #[must_use]
    pub fn with_match_indices(mut self, indices: Vec<usize>) -> Self {
        self.match_indices = indices;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_ordering() {
        let low = Score::new(10);
        let high = Score::new(100);
        assert!(high > low);
        assert_eq!(low.value(), 10);
        assert_eq!(Score::ZERO.value(), 0);
    }

    #[test]
    fn test_app_entry_builder() {
        let app = AppEntry::new("Visual Studio Code", "/usr/bin/code", AppSource::Path)
            .with_description("Code editing redefined")
            .with_keywords(["editor", "development"]);

        assert_eq!(app.name, "Visual Studio Code");
        assert_eq!(app.normalized_name, "visual studio code");
        assert_eq!(app.source, AppSource::Path);
        assert_eq!(app.description.as_deref(), Some("Code editing redefined"));
        assert_eq!(app.keywords, vec!["editor", "development"]);
    }

    #[test]
    fn test_command_entry_builder() {
        let cmd = CommandEntry::new("lock", "Lock Screen", "rundll32.exe")
            .with_args(["user32.dll,LockWorkStation"])
            .with_description("Locks the workstation");

        assert_eq!(cmd.id, "lock");
        assert_eq!(cmd.args, vec!["user32.dll,LockWorkStation"]);
    }

    #[test]
    fn test_search_result_constructors() {
        let app = AppEntry::new("Terminal", "/bin/bash", AppSource::Path);
        let result = SearchResult::from_app(app, Score::new(85)).with_match_indices(vec![0, 1, 2]);

        assert_eq!(result.title, "Terminal");
        assert_eq!(result.score, Score::new(85));
        assert_eq!(result.match_indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_traits_derivation() {
        let score = Score::new(42);
        let score_clone = score;
        assert_eq!(score, score_clone);
        assert_eq!(format!("{score:?}"), "Score(42)");
    }
}
