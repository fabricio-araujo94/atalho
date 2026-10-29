//! Stub implementations of platform traits for non-Windows platforms.
//!
//! Returns [`core::AppError::PlatformFailed`] indicating unsupported platform.

use core::{
    AppError, ForegroundManager, GlobalHotkey, HotkeyId, HotkeyModifiers, Hwnd, KeyCode, Result,
    WindowHelper,
};

/// Non-Windows stub for [`GlobalHotkey`].
#[derive(Debug, Default, Clone, Copy)]
pub struct StubGlobalHotkey;

impl StubGlobalHotkey {
    /// Creates a new [`StubGlobalHotkey`].
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl GlobalHotkey for StubGlobalHotkey {
    fn register(&mut self, _mods: HotkeyModifiers, _key: KeyCode) -> Result<()> {
        Err(AppError::unsupported_platform())
    }

    fn unregister(&mut self, _id: HotkeyId) -> Result<()> {
        Err(AppError::unsupported_platform())
    }
}

/// Non-Windows stub for [`WindowHelper`].
#[derive(Debug, Default, Clone, Copy)]
pub struct StubWindowHelper;

impl StubWindowHelper {
    /// Creates a new [`StubWindowHelper`].
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl WindowHelper for StubWindowHelper {
    fn set_borderless(&mut self, _hwnd: Hwnd) -> Result<()> {
        Err(AppError::unsupported_platform())
    }

    fn center_on_primary_monitor(&self) -> Result<()> {
        Err(AppError::unsupported_platform())
    }

    fn set_topmost(&self, _topmost: bool) -> Result<()> {
        Err(AppError::unsupported_platform())
    }

    fn focus(&self) -> Result<()> {
        Err(AppError::unsupported_platform())
    }
}

/// Non-Windows stub for [`ForegroundManager`].
#[derive(Debug, Default, Clone, Copy)]
pub struct StubForegroundManager;

impl StubForegroundManager {
    /// Creates a new [`StubForegroundManager`].
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ForegroundManager for StubForegroundManager {
    fn bring_to_front(&self, _hwnd: Hwnd) -> Result<()> {
        Err(AppError::unsupported_platform())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stubs_return_unsupported() {
        let mut hotkey = StubGlobalHotkey::new();
        assert!(
            hotkey
                .register(HotkeyModifiers::ALT, KeyCode::Space)
                .is_err()
        );
        assert!(hotkey.unregister(1).is_err());

        let mut window = StubWindowHelper::new();
        assert!(window.set_borderless(0).is_err());
        assert!(window.center_on_primary_monitor().is_err());
        assert!(window.set_topmost(true).is_err());
        assert!(window.focus().is_err());

        let fg = StubForegroundManager::new();
        assert!(fg.bring_to_front(0).is_err());
    }
}
