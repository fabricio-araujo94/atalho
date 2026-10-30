//! Desktop application shortcut indexer.

use core::{AppEntry, AppSource, Indexer, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

#[cfg(windows)]
use crate::shortcut::parse_shortcut;

/// Indexer that scans Desktop directories for application shortcuts (.lnk).
#[derive(Debug, Default, Clone)]
pub struct DesktopIndexer {
    custom_roots: Vec<PathBuf>,
}

impl DesktopIndexer {
    /// Creates a new [`DesktopIndexer`] scanning standard user and public desktop locations.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            custom_roots: Vec::new(),
        }
    }

    /// Creates a [`DesktopIndexer`] pointing to custom directory paths (useful for testing).
    #[must_use]
    pub const fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            custom_roots: roots,
        }
    }

    /// Resolves the desktop directories to scan.
    #[must_use]
    pub fn resolve_roots(&self) -> Vec<PathBuf> {
        if !self.custom_roots.is_empty() {
            return self.custom_roots.clone();
        }

        let mut roots = Vec::new();

        // 1. User Desktop via USERPROFILE or HOME
        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let user_desktop = PathBuf::from(user_profile).join("Desktop");
            if user_desktop.exists() {
                roots.push(user_desktop);
            }
        } else if let Ok(home) = std::env::var("HOME") {
            let user_desktop = PathBuf::from(home).join("Desktop");
            if user_desktop.exists() {
                roots.push(user_desktop);
            }
        }

        // 2. Public Desktop on Windows
        if let Ok(public_dir) = std::env::var("PUBLIC") {
            let public_desktop = PathBuf::from(public_dir).join("Desktop");
            if public_desktop.exists() {
                roots.push(public_desktop);
            }
        }

        roots
    }
}

/// Recursively scans a desktop directory for `.lnk` shortcut files using async I/O.
async fn collect_desktop_shortcuts(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(read_dir) => read_dir,
        Err(err) => {
            warn!("Failed to read desktop directory {}: {err}", dir.display());
            return;
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let file_type = match entry.file_type().await {
            Ok(ft) => ft,
            Err(err) => {
                debug!("Failed to query file type for {}: {err}", path.display());
                continue;
            }
        };

        if file_type.is_file()
            && let Some(ext) = path.extension()
            && ext.eq_ignore_ascii_case("lnk")
        {
            out.push(path);
        }
    }
}

#[async_trait::async_trait]
impl Indexer for DesktopIndexer {
    async fn scan(&self) -> Result<Vec<AppEntry>> {
        let roots = self.resolve_roots();
        let mut lnk_paths = Vec::new();

        for root in &roots {
            collect_desktop_shortcuts(root, &mut lnk_paths).await;
        }

        let mut apps = Vec::new();

        for lnk_path in lnk_paths {
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
                        if !info.target_path.exists() {
                            debug!(
                                "Skipping desktop shortcut {}: target {} does not exist",
                                lnk_path.display(),
                                info.target_path.display()
                            );
                            continue;
                        }

                        let mut entry =
                            AppEntry::new(file_stem, info.target_path, AppSource::Desktop);
                        if let Some(args) = info.arguments {
                            entry = entry.with_arguments(args);
                        }
                        if let Some(icon) = info.icon_path {
                            entry = entry.with_icon(icon);
                        }
                        apps.push(entry);
                    }
                    Ok(Err(err)) => {
                        debug!(
                            "Failed to parse desktop shortcut {}: {err}",
                            lnk_path.display()
                        );
                    }
                    Err(join_err) => {
                        warn!(
                            "Desktop shortcut parsing task panicked for {}: {join_err}",
                            lnk_path.display()
                        );
                    }
                }
            }

            #[cfg(not(windows))]
            {
                let canonical_target = dunce::canonicalize(&lnk_path).unwrap_or(lnk_path);
                let entry = AppEntry::new(file_stem, canonical_target, AppSource::Desktop);
                apps.push(entry);
            }
        }

        Ok(apps)
    }
}
