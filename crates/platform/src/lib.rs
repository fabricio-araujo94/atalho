//! Operating system abstraction and platform-specific integrations for Atalho.
//!
//! Provides platform discovery, native process launch utilities, and Windows API
//! integrations gated behind `cfg(windows)`.

use core::{
    AppError, ForegroundManager, GlobalHotkey, HotkeyId, HotkeyModifiers, Hwnd, KeyCode, Result,
    WindowHelper,
};
use std::path::{Path, PathBuf};

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{
    WindowsForegroundManager, WindowsGlobalHotkey, WindowsWindowHelper, to_win32_modifiers,
    to_win32_vk, win32_result,
};

#[cfg(not(windows))]
pub mod stub;

#[cfg(not(windows))]
pub use stub::{StubForegroundManager, StubGlobalHotkey, StubWindowHelper};

#[cfg(windows)]
mod windows_impl {
    use std::path::PathBuf;

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
#[derive(Debug, Default, Clone)]
pub struct PlatformManager {
    #[cfg(windows)]
    hotkey: WindowsGlobalHotkey,
    #[cfg(windows)]
    window: WindowsWindowHelper,
    #[cfg(windows)]
    foreground: WindowsForegroundManager,

    #[cfg(not(windows))]
    hotkey: StubGlobalHotkey,
    #[cfg(not(windows))]
    window: StubWindowHelper,
    #[cfg(not(windows))]
    foreground: StubForegroundManager,
}

impl PlatformManager {
    /// Creates a new [`PlatformManager`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            #[cfg(windows)]
            hotkey: WindowsGlobalHotkey::new(),
            #[cfg(windows)]
            window: WindowsWindowHelper::new(),
            #[cfg(windows)]
            foreground: WindowsForegroundManager::new(),

            #[cfg(not(windows))]
            hotkey: StubGlobalHotkey::new(),
            #[cfg(not(windows))]
            window: StubWindowHelper::new(),
            #[cfg(not(windows))]
            foreground: StubForegroundManager::new(),
        }
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

        cmd.spawn().map(|_| ()).map_err(|err| {
            AppError::platform_failed(format!("Failed to spawn {}: {err}", path.display()))
        })
    }
}

impl GlobalHotkey for PlatformManager {
    fn register(&mut self, mods: HotkeyModifiers, key: KeyCode) -> Result<()> {
        self.hotkey.register(mods, key)
    }

    fn unregister(&mut self, id: HotkeyId) -> Result<()> {
        self.hotkey.unregister(id)
    }
}

impl WindowHelper for PlatformManager {
    fn set_borderless(&mut self, hwnd: Hwnd) -> Result<()> {
        self.window.set_borderless(hwnd)
    }

    fn center_on_primary_monitor(&self) -> Result<()> {
        self.window.center_on_primary_monitor()
    }

    fn set_topmost(&self, topmost: bool) -> Result<()> {
        self.window.set_topmost(topmost)
    }

    fn focus(&self) -> Result<()> {
        self.window.focus()
    }
}

impl ForegroundManager for PlatformManager {
    fn bring_to_front(&self, hwnd: Hwnd) -> Result<()> {
        self.foreground.bring_to_front(hwnd)
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
    }
}
