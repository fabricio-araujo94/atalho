//! System PATH application indexer.

use core::{AppEntry, AppError, AppSource, Indexer, Result};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
use tracing::debug;

/// Set of supported executable file extensions on Windows.
const EXECUTABLE_EXTENSIONS: &[&str] = &["exe", "cmd", "bat", "msc"];

/// Indexer that scans directories listed in the system `%PATH%` environment variable.
#[derive(Debug, Default, Clone)]
pub struct PathIndexer {
    custom_paths: Vec<PathBuf>,
}

impl PathIndexer {
    /// Creates a new [`PathIndexer`] scanning directories from the current system `%PATH%`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            custom_paths: Vec::new(),
        }
    }

    /// Creates a [`PathIndexer`] pointing to custom directory paths (useful for testing).
    #[must_use]
    pub const fn with_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            custom_paths: paths,
        }
    }

    /// Resolves the list of directory paths to scan.
    #[must_use]
    pub fn resolve_paths(&self) -> Vec<PathBuf> {
        if !self.custom_paths.is_empty() {
            return self.custom_paths.clone();
        }

        std::env::var_os("PATH")
            .map(|var| std::env::split_paths(&var).filter(|p| p.is_dir()).collect())
            .unwrap_or_default()
    }

    /// Checks if a file path has an executable extension matching the target platform.
    #[must_use]
    pub fn is_executable_file(path: &std::path::Path) -> bool {
        path.extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| {
                EXECUTABLE_EXTENSIONS
                    .iter()
                    .any(|&expected| ext.eq_ignore_ascii_case(expected))
            })
    }
}

#[async_trait::async_trait]
impl Indexer for PathIndexer {
    async fn scan(&self) -> Result<Vec<AppEntry>> {
        Ok(self
            .scan_with_cancellation(CancellationToken::new())
            .await?
            .unwrap_or_default())
    }

    async fn scan_with_cancellation(
        &self,
        token: CancellationToken,
    ) -> Result<Option<Vec<AppEntry>>> {
        let indexer = self.clone();
        let directories = tokio::task::spawn_blocking(move || indexer.resolve_paths())
            .await
            .map_err(|err| {
                AppError::execution_failed(format!("PATH resolution task failed: {err}"))
            })?;
        let mut apps = Vec::new();

        for dir in directories {
            if token.is_cancelled() {
                return Ok(None);
            }
            let mut read_dir = match tokio::fs::read_dir(&dir).await {
                Ok(rd) => rd,
                Err(err) => {
                    debug!("Failed to read PATH directory {}: {err}", dir.display());
                    continue;
                }
            };

            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if token.is_cancelled() {
                    return Ok(None);
                }
                let path = entry.path();
                let file_type = match entry.file_type().await {
                    Ok(ft) => ft,
                    Err(err) => {
                        debug!("Failed to query file type for {}: {err}", path.display());
                        continue;
                    }
                };

                if !file_type.is_file() {
                    continue;
                }

                if Self::is_executable_file(&path) {
                    let file_stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default()
                        .to_string();

                    if file_stem.is_empty() {
                        continue;
                    }

                    let canonical = tokio::fs::canonicalize(&path).await.unwrap_or(path);
                    apps.push(AppEntry::new(file_stem, canonical, AppSource::Path));
                }
            }
        }

        Ok(Some(apps))
    }
}
