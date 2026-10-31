//! Windows Start Menu application indexer.

use core::{AppEntry, AppError, AppSource, Indexer, Result};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

#[cfg(windows)]
use crate::shortcut::parse_shortcut;

/// Indexer that scans Windows Start Menu directories for shortcut (.lnk) files.
#[derive(Debug, Default, Clone)]
pub struct StartMenuIndexer {
    custom_roots: Vec<PathBuf>,
}

impl StartMenuIndexer {
    /// Creates a new [`StartMenuIndexer`] scanning standard Windows Start Menu locations.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            custom_roots: Vec::new(),
        }
    }

    /// Creates a [`StartMenuIndexer`] pointing to custom root directories (useful for testing).
    #[must_use]
    pub const fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            custom_roots: roots,
        }
    }

    /// Resolves the root directories to scan for Start Menu shortcuts.
    #[must_use]
    pub fn resolve_roots(&self) -> Vec<PathBuf> {
        if !self.custom_roots.is_empty() {
            return self.custom_roots.clone();
        }

        let mut roots = Vec::new();

        if let Ok(app_data) = std::env::var("APPDATA") {
            let user_programs = PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            if user_programs.exists() {
                roots.push(user_programs);
            }
        }

        if let Ok(program_data) = std::env::var("PROGRAMDATA") {
            let common_programs = PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            if common_programs.exists() {
                roots.push(common_programs);
            }
        }

        roots
    }
}

/// Recursively scans a directory for `.lnk` shortcut files using async I/O.
async fn collect_lnk_files(dir: &Path, out: &mut Vec<PathBuf>, token: &CancellationToken) -> bool {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(read_dir) => read_dir,
        Err(err) => {
            warn!("Failed to read directory {}: {err}", dir.display());
            return true;
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        if token.is_cancelled() {
            return false;
        }
        let path = entry.path();
        let file_type = match entry.file_type().await {
            Ok(ft) => ft,
            Err(err) => {
                debug!("Failed to query file type for {}: {err}", path.display());
                continue;
            }
        };

        if file_type.is_dir() {
            if !Box::pin(collect_lnk_files(&path, out, token)).await {
                return false;
            }
        } else if file_type.is_file()
            && let Some(ext) = path.extension()
            && ext.eq_ignore_ascii_case("lnk")
        {
            out.push(path);
        }
    }
    true
}

#[async_trait::async_trait]
impl Indexer for StartMenuIndexer {
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
        let roots = tokio::task::spawn_blocking(move || indexer.resolve_roots())
            .await
            .map_err(|err| {
                AppError::execution_failed(format!("Start Menu resolution task failed: {err}"))
            })?;
        let mut lnk_paths = Vec::new();

        for root in &roots {
            if !collect_lnk_files(root, &mut lnk_paths, &token).await {
                return Ok(None);
            }
        }

        let mut apps = Vec::new();

        for lnk_path in lnk_paths {
            if token.is_cancelled() {
                return Ok(None);
            }
            let file_stem = lnk_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            if file_stem.is_empty() {
                continue;
            }

            #[cfg(windows)]
            {
                let lnk_copy = lnk_path.clone();
                let parse_res =
                    tokio::task::spawn_blocking(move || parse_shortcut(&lnk_copy)).await;

                match parse_res {
                    Ok(Ok(info)) => {
                        if !tokio::fs::try_exists(&info.target_path)
                            .await
                            .unwrap_or(false)
                        {
                            debug!(
                                "Skipping shortcut {}: target {} does not exist",
                                lnk_path.display(),
                                info.target_path.display()
                            );
                            continue;
                        }

                        let mut entry =
                            AppEntry::new(file_stem, info.target_path, AppSource::StartMenu);
                        if let Some(args) = info.arguments {
                            entry = entry.with_arguments(args);
                        }
                        if let Some(icon) = info.icon_path {
                            entry = entry.with_icon(icon);
                        }
                        apps.push(entry);
                    }
                    Ok(Err(err)) => {
                        debug!("Failed to parse shortcut {}: {err}", lnk_path.display());
                    }
                    Err(join_err) => {
                        warn!(
                            "Shortcut parsing task panicked for {}: {join_err}",
                            lnk_path.display()
                        );
                    }
                }
            }

            #[cfg(not(windows))]
            {
                // Non-Windows stub for testing / CI
                let canonical_target = tokio::fs::canonicalize(&lnk_path).await.unwrap_or(lnk_path);
                let entry = AppEntry::new(file_stem, canonical_target, AppSource::StartMenu);
                apps.push(entry);
            }
        }

        Ok(Some(apps))
    }
}
