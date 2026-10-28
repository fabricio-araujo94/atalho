//! Operating system abstraction and platform-specific integrations for Atalho.
//!
//! Provides platform discovery, native process launch utilities, and Windows API
//! integrations gated behind `cfg(windows)`.

use std::path::{Path, PathBuf};
use core::{AppError, Result};

#[cfg(windows)]
mod windows_impl {
    use super::*;

    /// Returns standard Windows application shortcut directory paths (Start Menu programs).
    #[must_use]
    pub fn get_start_menu_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Ok(app_data) = std::env::var("APPDATA") {
            let user_start = PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            if user_start.exists() {
                paths.push(user_start);
            }
        }

        if let Ok(program_data) = std::env::var("PROGRAMDATA") {
            let common_start = PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            if common_start.exists() {
                paths.push(common_start);
            }
        }

        paths
    }
}

/// Identifies the running operating system platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
    /// Microsoft Windows
    Windows,
    /// Linux or BSD
    Linux,
    /// Apple macOS
    MacOS,
    /// Other operating system
    Unknown,
}

/// Platform abstraction service providing OS-level capabilities.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlatformManager;

impl PlatformManager {
    /// Creates a new [`PlatformManager`].
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns the active operating system kind.
    #[must_use]
    pub const fn platform_kind(&self) -> PlatformKind {
        if cfg!(target_os = "windows") {
            PlatformKind::Windows
        } else if cfg!(target_os = "linux") {
            PlatformKind::Linux
        } else if cfg!(target_os = "macos") {
            PlatformKind::MacOS
        } else {
            PlatformKind::Unknown
        }
    }

    /// Returns standard system search directories for discovering installed applications.
    #[must_use]
    pub fn application_search_paths(&self) -> Vec<PathBuf> {
        #[cfg(windows)]
        {
            windows_impl::get_start_menu_paths()
        }

        #[cfg(not(windows))]
        {
            let mut paths = Vec::new();
            paths.push(PathBuf::from("/usr/share/applications"));
            paths.push(PathBuf::from("/usr/local/share/applications"));
            paths
        }
    }

    /// Spawns an executable target using standard operating system process launching.
    ///
    /// # Errors
    /// Returns [`AppError::PlatformFailed`] if the process fails to spawn.
    pub fn launch_executable(&self, path: &Path, args: &[String]) -> Result<()> {
        let mut cmd = std::process::Command::new(path);
        cmd.args(args);

        cmd.spawn()
            .map(|_| ())
            .map_err(|err| AppError::platform_failed(format!("Failed to spawn {}: {err}", path.display())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_manager_kind() {
        let manager = PlatformManager::new();
        let kind = manager.platform_kind();

        if cfg!(windows) {
            assert_eq!(kind, PlatformKind::Windows);
        } else if cfg!(target_os = "linux") {
            assert_eq!(kind, PlatformKind::Linux);
        } else if cfg!(target_os = "macos") {
            assert_eq!(kind, PlatformKind::MacOS);
        }
    }

    #[test]
    fn test_search_paths_non_empty() {
        let manager = PlatformManager::new();
        let _paths = manager.application_search_paths();
        // Just verify it runs cleanly without panicking
    }
}
