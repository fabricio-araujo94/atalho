//! Platform abstraction contracts and types for native window and input management.

use crate::error::Result;

/// Native window handle identifier (corresponding to Win32 `HWND`).
pub type Hwnd = isize;

/// Identifier assigned to a registered global hotkey.
pub type HotkeyId = i32;

/// Modifier key combinations used for global hotkey registration.
#[allow(clippy::struct_excessive_bools)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub struct HotkeyModifiers {
    /// Alt key modifier.
    pub alt: bool,
    /// Control key modifier.
    pub ctrl: bool,
    /// Shift key modifier.
    pub shift: bool,
    /// Windows/Super key modifier.
    pub win: bool,
}

impl HotkeyModifiers {
    /// No modifier keys.
    pub const NONE: Self = Self {
        alt: false,
        ctrl: false,
        shift: false,
        win: false,
    };

    /// Alt key modifier.
    pub const ALT: Self = Self {
        alt: true,
        ctrl: false,
        shift: false,
        win: false,
    };

    /// Control key modifier.
    pub const CTRL: Self = Self {
        alt: false,
        ctrl: true,
        shift: false,
        win: false,
    };

    /// Shift key modifier.
    pub const SHIFT: Self = Self {
        alt: false,
        ctrl: false,
        shift: true,
        win: false,
    };

    /// Windows/Super key modifier.
    pub const WIN: Self = Self {
        alt: false,
        ctrl: false,
        shift: false,
        win: true,
    };
}

/// Convenient type alias for hotkey modifiers.
pub type Mods = HotkeyModifiers;

/// Key code abstraction for hotkey combinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum KeyCode {
    /// Spacebar key.
    Space,
    /// Return / Enter key.
    Return,
    /// Escape key.
    Escape,
    /// Tab key.
    Tab,
    /// Backspace key.
    Backspace,
    /// Alphanumeric or punctuation character.
    Char(char),
    /// Function keys (F1 - F24).
    F(u8),
    /// Platform-specific raw virtual-key code.
    Raw(u32),
}

/// Convenient type alias for key code.
pub type Key = KeyCode;

/// Contract for managing system-wide keyboard shortcuts.
pub trait GlobalHotkey: Send + Sync {
    /// Registers a global hotkey with the specified modifier keys and key code.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::PlatformFailed`] if registration fails (e.g. key already registered).
    fn register(&mut self, mods: HotkeyModifiers, key: KeyCode) -> Result<()>;

    /// Unregisters the global hotkey associated with `id`.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::PlatformFailed`] if unregistration fails.
    fn unregister(&mut self, id: HotkeyId) -> Result<()>;
}

/// Contract for managing window styling, positioning, and focus.
pub trait WindowHelper: Send + Sync {
    /// Configures the specified window handle to be borderless and layered/toolwindow.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::PlatformFailed`] if modifying window styles fails.
    fn set_borderless(&mut self, hwnd: Hwnd) -> Result<()>;

    /// Centers the window on the primary display monitor.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::PlatformFailed`] if monitor detection or positioning fails.
    fn center_on_primary_monitor(&self) -> Result<()>;

    /// Toggles the topmost (always-on-top) state of the window.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::PlatformFailed`] if setting window z-order fails.
    fn set_topmost(&self, topmost: bool) -> Result<()>;

    /// Directs keyboard input focus to the window.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::PlatformFailed`] if setting focus fails.
    fn focus(&self) -> Result<()>;
}

/// Contract for managing foreground window activation and bypassing focus restrictions.
pub trait ForegroundManager: Send + Sync {
    /// Brings the specified window handle to the foreground and activates it.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::PlatformFailed`] if bringing the window to the front fails.
    fn bring_to_front(&self, hwnd: Hwnd) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey_modifiers() {
        const { assert!(HotkeyModifiers::ALT.alt) };
        const { assert!(!HotkeyModifiers::ALT.ctrl) };
        assert_eq!(HotkeyModifiers::NONE, HotkeyModifiers::default());
    }

    #[test]
    fn test_key_code_variants() {
        assert_eq!(KeyCode::Space, KeyCode::Space);
        assert_ne!(KeyCode::Space, KeyCode::Return);
        assert_eq!(KeyCode::Char('A'), KeyCode::Char('A'));
    }
}
