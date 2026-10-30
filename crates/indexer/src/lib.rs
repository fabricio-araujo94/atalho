//! Application and command indexing engine for Atalho.
//!
//! Discovers launchable applications from Windows Start Menu, PATH, and Desktop sources,
//! producing normalized and deduplicated lists of [`core::AppEntry`].

pub mod composite;
pub mod desktop;
pub mod path;
pub mod shortcut;
pub mod start_menu;

pub use composite::CompositeIndexer;
pub use desktop::DesktopIndexer;
pub use path::PathIndexer;
pub use shortcut::ShortcutInfo;
pub use start_menu::StartMenuIndexer;

use core::{AppEntry, CommandEntry, Indexer, Result};

/// Default application indexer combining concrete system indexers with optional in-memory caching.
#[derive(Debug, Default, Clone)]
pub struct DefaultIndexer {
    composite: CompositeIndexer,
    preloaded_apps: Vec<AppEntry>,
}

impl DefaultIndexer {
    /// Creates a new, empty [`DefaultIndexer`] backed by [`CompositeIndexer`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new [`DefaultIndexer`] pre-populated with provided entries (useful for testing).
    #[must_use]
    pub fn with_entries(apps: Vec<AppEntry>, _commands: Vec<CommandEntry>) -> Self {
        Self {
            composite: CompositeIndexer::default(),
            preloaded_apps: apps,
        }
    }
}

#[async_trait::async_trait]
impl Indexer for DefaultIndexer {
    async fn scan(&self) -> Result<Vec<AppEntry>> {
        if !self.preloaded_apps.is_empty() {
            return Ok(self.preloaded_apps.clone());
        }
        self.composite.scan().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::AppSource;
    use std::path::PathBuf;
    use std::sync::Arc;

    struct MockIndexer {
        apps: Vec<AppEntry>,
    }

    #[async_trait::async_trait]
    impl Indexer for MockIndexer {
        async fn scan(&self) -> Result<Vec<AppEntry>> {
            Ok(self.apps.clone())
        }
    }

    #[tokio::test]
    async fn test_composite_indexer_deduplication() {
        let path = PathBuf::from("C:\\Windows\\notepad.exe");
        let app1 = AppEntry::new("Notepad", &path, AppSource::StartMenu);
        let app2 = AppEntry::new("notepad", &path, AppSource::Path);
        let app3 = AppEntry::new("Calculator", "C:\\Windows\\calc.exe", AppSource::StartMenu);

        let indexer1 = MockIndexer {
            apps: vec![app1.clone()],
        };
        let indexer2 = MockIndexer {
            apps: vec![app2.clone(), app3.clone()],
        };

        let composite = CompositeIndexer::new(vec![Arc::new(indexer1), Arc::new(indexer2)]);
        let results = composite.scan().await.unwrap_or_default();

        // Notepad should be deduplicated (StartMenu preferred over Path)
        assert_eq!(results.len(), 2);
        let notepad_entry = results.iter().find(|a| a.normalized_name == "notepad");
        assert!(notepad_entry.is_some());
        if let Some(entry) = notepad_entry {
            assert_eq!(entry.source, AppSource::StartMenu);
        }
    }

    #[tokio::test]
    async fn test_path_indexer_scans_executables() {
        let temp_dir = std::env::temp_dir().join("atalho_test_path_indexer");
        let _ = tokio::fs::create_dir_all(&temp_dir).await;

        let exe_file = temp_dir.join("test_app.exe");
        let bat_file = temp_dir.join("run_script.bat");
        let txt_file = temp_dir.join("readme.txt");

        let _ = tokio::fs::write(&exe_file, b"binary").await;
        let _ = tokio::fs::write(&bat_file, b"@echo off").await;
        let _ = tokio::fs::write(&txt_file, b"notes").await;

        let indexer = PathIndexer::with_paths(vec![temp_dir.clone()]);
        let results = indexer.scan().await.unwrap_or_default();

        let names: Vec<String> = results.into_iter().map(|a| a.name).collect();
        assert!(names.contains(&"test_app".to_string()));
        assert!(names.contains(&"run_script".to_string()));
        assert!(!names.contains(&"readme".to_string()));

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
}
