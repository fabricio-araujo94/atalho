//! User-configurable launcher settings.

use crate::models::AppSource;
use serde::{Deserialize, Serialize};

/// Application color theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeName {
    /// Dark launcher theme.
    #[default]
    Dark,
    /// Light launcher theme.
    Light,
    /// Solarized color palette.
    Solarized,
}

/// Enables or disables one application discovery source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceToggle {
    /// Discovery source to configure.
    pub source: AppSource,
    /// Whether this source is included during indexing.
    pub enabled: bool,
}

/// Persistent user preferences for the launcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Global launcher hotkey.
    pub hotkey: String,
    /// Maximum number of search results to display.
    pub max_results: usize,
    /// Active color theme.
    pub theme: ThemeName,
    /// Enabled application discovery sources.
    pub index_sources: Vec<SourceToggle>,
    /// Search query debounce window in milliseconds.
    pub debounce_ms: u64,
    /// Whether the launcher starts when the current user signs in.
    pub auto_start: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: "Alt+Space".to_owned(),
            max_results: 8,
            theme: ThemeName::Dark,
            index_sources: vec![
                SourceToggle {
                    source: AppSource::StartMenu,
                    enabled: true,
                },
                SourceToggle {
                    source: AppSource::Path,
                    enabled: true,
                },
                SourceToggle {
                    source: AppSource::Desktop,
                    enabled: true,
                },
                SourceToggle {
                    source: AppSource::UserPinned,
                    enabled: true,
                },
            ],
            debounce_ms: 80,
            auto_start: false,
        }
    }
}
