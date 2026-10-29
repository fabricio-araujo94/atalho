//! Windows-specific platform primitives implementing global hotkeys, window styling,
//! centering on primary monitor, and foreground activation.

use core::{
    AppError, ForegroundManager, GlobalHotkey, HotkeyId, HotkeyModifiers, Hwnd, KeyCode, Result,
    WindowHelper,
};
use std::mem::size_of;
use windows::Win32::Foundation::{BOOL, GetLastError, HWND, RECT, SetLastError, WIN32_ERROR};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTOPRIMARY, MONITOR_FROM_FLAGS, MONITORINFO,
    MonitorFromWindow,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MOD_ALT,
    MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, SetFocus, UnregisterHotKey,
    VK_ESCAPE, VK_F1, VK_MENU, VK_RETURN, VK_SPACE, VK_TAB, keybd_event,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GWL_EXSTYLE, GWL_STYLE, GetWindowRect, HWND_NOTOPMOST, HWND_TOPMOST,
    SET_WINDOW_POS_FLAGS, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowPos, WINDOW_LONG_PTR_INDEX, WS_CAPTION, WS_EX_LAYERED,
    WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};

#[cfg(target_pointer_width = "64")]
use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW};

#[cfg(target_pointer_width = "32")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW as GetWindowLongPtrW, SetWindowLongW as SetWindowLongPtrW,
};

// ============================================================================
// Dedicated Raw Unsafe Functions (Isolated Win32 Boundaries)
// ============================================================================

/// Raw Win32 call to `RegisterHotKey`.
// SAFETY: Registers a system-wide hotkey identifier. Passing a null HWND binds the hotkey
// to the message queue of the calling thread.
unsafe fn raw_register_hotkey(
    hwnd: HWND,
    id: i32,
    fs_modifiers: HOT_KEY_MODIFIERS,
    vk: u32,
) -> windows::core::Result<()> {
    // SAFETY: Calling Win32 RegisterHotKey with verified arguments.
    unsafe { RegisterHotKey(hwnd, id, fs_modifiers, vk) }
}

/// Raw Win32 call to `UnregisterHotKey`.
// SAFETY: Unregisters a hotkey identifier from the message queue. Safe for any arbitrary ID.
unsafe fn raw_unregister_hotkey(hwnd: HWND, id: i32) -> windows::core::Result<()> {
    // SAFETY: Calling Win32 UnregisterHotKey for caller thread.
    unsafe { UnregisterHotKey(hwnd, id) }
}

/// Raw Win32 call to `GetWindowLongPtrW`.
// SAFETY: Queries window style or extended style attributes for the supplied window handle.
unsafe fn raw_get_window_long_ptr(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> isize {
    // SAFETY: Calling Win32 GetWindowLongPtrW for the target HWND.
    unsafe { GetWindowLongPtrW(hwnd, index) }
}

/// Raw Win32 call to `SetWindowLongPtrW`.
// SAFETY: Modifies window styling attributes for the supplied window handle.
unsafe fn raw_set_window_long_ptr(
    hwnd: HWND,
    index: WINDOW_LONG_PTR_INDEX,
    new_val: isize,
) -> isize {
    // SAFETY: Calling Win32 SetWindowLongPtrW for the target HWND.
    unsafe { SetWindowLongPtrW(hwnd, index, new_val) }
}

/// Raw Win32 call to `SetWindowPos`.
// SAFETY: Repositions, resizes, or updates z-order for the specified HWND.
unsafe fn raw_set_window_pos(
    hwnd: HWND,
    insert_after: HWND,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    flags: SET_WINDOW_POS_FLAGS,
) -> windows::core::Result<()> {
    // SAFETY: Calling Win32 SetWindowPos with verified coordinates and flags.
    unsafe { SetWindowPos(hwnd, insert_after, x, y, cx, cy, flags) }
}

/// Raw Win32 call to `MonitorFromWindow`.
// SAFETY: Queries the HMONITOR associated with a window. Uses MONITOR_DEFAULTTOPRIMARY fallback
// if the window handle is unmapped or offscreen.
unsafe fn raw_monitor_from_window(hwnd: HWND, flags: MONITOR_FROM_FLAGS) -> HMONITOR {
    // SAFETY: Calling Win32 MonitorFromWindow with default fallback flag.
    unsafe { MonitorFromWindow(hwnd, flags) }
}

/// Raw Win32 call to `GetMonitorInfoW`.
// SAFETY: Populates the caller-provided pointer to an initialized MONITORINFO structure.
unsafe fn raw_get_monitor_info(hmonitor: HMONITOR, info: *mut MONITORINFO) -> BOOL {
    // SAFETY: Calling Win32 GetMonitorInfoW with valid pointers.
    unsafe { GetMonitorInfoW(hmonitor, info) }
}

/// Raw Win32 call to `GetWindowRect`.
// SAFETY: Reads current screen dimensions into caller-provided RECT pointer.
unsafe fn raw_get_window_rect(hwnd: HWND, rect: *mut RECT) -> windows::core::Result<()> {
    // SAFETY: Calling Win32 GetWindowRect with valid RECT pointer.
    unsafe { GetWindowRect(hwnd, rect) }
}

/// Raw Win32 call to `SetForegroundWindow`.
// SAFETY: Requests operating system window activation and foreground focus transfer.
unsafe fn raw_set_foreground_window(hwnd: HWND) -> BOOL {
    // SAFETY: Calling Win32 SetForegroundWindow.
    unsafe { SetForegroundWindow(hwnd) }
}

/// Raw Win32 call to `BringWindowToTop`.
// SAFETY: Moves window to top of overlapping z-order stack.
unsafe fn raw_bring_window_to_top(hwnd: HWND) -> windows::core::Result<()> {
    // SAFETY: Calling Win32 BringWindowToTop.
    unsafe { BringWindowToTop(hwnd) }
}

/// Raw Win32 call to `SetFocus`.
// SAFETY: Directs keyboard focus to the target window within the current thread queue.
unsafe fn raw_set_focus(hwnd: HWND) -> windows::core::Result<HWND> {
    // SAFETY: Calling Win32 SetFocus.
    unsafe { SetFocus(hwnd) }
}

/// Raw Win32 call to `keybd_event`.
// SAFETY: Synthesizes virtual key events into the system input queue.
unsafe fn raw_keybd_event(b_vk: u8, b_scan: u8, dw_flags: KEYBD_EVENT_FLAGS, dw_extra_info: usize) {
    // SAFETY: Calling Win32 keybd_event.
    unsafe { keybd_event(b_vk, b_scan, dw_flags, dw_extra_info) };
}

/// Raw Win32 call to `GetLastError`.
// SAFETY: Reads thread-local error status without mutating state.
unsafe fn raw_get_last_error() -> WIN32_ERROR {
    // SAFETY: Calling Win32 GetLastError.
    unsafe { GetLastError() }
}

/// Raw Win32 call to `SetLastError`.
// SAFETY: Clears or sets thread-local error status.
unsafe fn raw_set_last_error(err: WIN32_ERROR) {
    // SAFETY: Calling Win32 SetLastError.
    unsafe { SetLastError(err) };
}

// ============================================================================
// Win32 Result Helper
// ============================================================================

/// Trait converting Win32 result types into domain [`core::Result`].
pub trait IntoWin32Result {
    /// Return value type on successful Win32 execution.
    type Output;

    /// Evaluates the Win32 call result, capturing `GetLastError()` on failure.
    ///
    /// # Errors
    /// Returns [`AppError::PlatformFailed`] if the Win32 call failed.
    fn into_win32_result(self, context: &str) -> Result<Self::Output>;
}

impl IntoWin32Result for BOOL {
    type Output = ();

    fn into_win32_result(self, context: &str) -> Result<()> {
        if self.as_bool() {
            Ok(())
        } else {
            // SAFETY: Reading last error from current thread is thread-safe and non-mutating.
            let err = unsafe { raw_get_last_error() };
            Err(AppError::platform_failed(format!(
                "{context} failed: Win32 error code {:#06X} ({})",
                err.0, err.0
            )))
        }
    }
}

impl<T> IntoWin32Result for windows::core::Result<T> {
    type Output = T;

    fn into_win32_result(self, context: &str) -> Result<T> {
        self.map_err(|err| AppError::platform_failed(format!("{context} failed: {err}")))
    }
}

/// Evaluates a Win32 call result, returning `Ok(())` on success or extracting
/// `GetLastError()` into [`AppError::PlatformFailed`] with descriptive context.
///
/// # Errors
/// Returns [`AppError::PlatformFailed`] if the Win32 call fails.
pub fn win32_result<R: IntoWin32Result>(result: R, context: &str) -> Result<R::Output> {
    result.into_win32_result(context)
}

// ============================================================================
// Modifier & Key Code Mapping
// ============================================================================

/// Converts domain [`HotkeyModifiers`] into Win32 [`HOT_KEY_MODIFIERS`] bitflags.
#[must_use]
pub fn to_win32_modifiers(mods: HotkeyModifiers) -> HOT_KEY_MODIFIERS {
    let mut flags = HOT_KEY_MODIFIERS(0);
    if mods.alt {
        flags |= MOD_ALT;
    }
    if mods.ctrl {
        flags |= MOD_CONTROL;
    }
    if mods.shift {
        flags |= MOD_SHIFT;
    }
    if mods.win {
        flags |= MOD_WIN;
    }
    // Prevent repeated WM_HOTKEY events when holding the combination down
    flags | MOD_NOREPEAT
}

/// Maps domain [`KeyCode`] to Win32 Virtual-Key code.
#[must_use]
pub fn to_win32_vk(key: KeyCode) -> u32 {
    match key {
        KeyCode::Space => u32::from(VK_SPACE.0),
        KeyCode::Return => u32::from(VK_RETURN.0),
        KeyCode::Escape => u32::from(VK_ESCAPE.0),
        KeyCode::Tab => u32::from(VK_TAB.0),
        KeyCode::Backspace => 0x08, // VK_BACK
        KeyCode::Char(c) => u32::from(c.to_ascii_uppercase() as u8),
        KeyCode::F(n) if (1..=24).contains(&n) => u32::from(VK_F1.0) + u32::from(n - 1),
        KeyCode::F(_) => u32::from(VK_F1.0),
        KeyCode::Raw(code) => code,
    }
}

// ============================================================================
// Windows Global Hotkey Implementation
// ============================================================================

/// Windows global hotkey manager leveraging `RegisterHotKey` and `UnregisterHotKey`.
#[derive(Debug, Default, Clone)]
pub struct WindowsGlobalHotkey {
    registered_ids: Vec<HotkeyId>,
}

impl WindowsGlobalHotkey {
    /// Default hotkey identifier reserved for the primary launcher activation.
    pub const DEFAULT_HOTKEY_ID: HotkeyId = 1;

    /// Creates a new [`WindowsGlobalHotkey`] instance.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            registered_ids: Vec::new(),
        }
    }

    /// Registers the default launcher activation hotkey (Alt + Space).
    ///
    /// # Errors
    /// Returns [`AppError::PlatformFailed`] if registration fails.
    pub fn register_default(&mut self) -> Result<()> {
        self.register(HotkeyModifiers::ALT, KeyCode::Space)
    }
}

impl GlobalHotkey for WindowsGlobalHotkey {
    fn register(&mut self, mods: HotkeyModifiers, key: KeyCode) -> Result<()> {
        let id = Self::DEFAULT_HOTKEY_ID;
        let fs_modifiers = to_win32_modifiers(mods);
        let vk = to_win32_vk(key);

        // SAFETY: Binding to thread message queue via null HWND.
        let raw_result =
            unsafe { raw_register_hotkey(HWND(std::ptr::null_mut()), id, fs_modifiers, vk) };
        win32_result(raw_result, "RegisterHotKey")?;

        if !self.registered_ids.contains(&id) {
            self.registered_ids.push(id);
        }
        Ok(())
    }

    fn unregister(&mut self, id: HotkeyId) -> Result<()> {
        // SAFETY: Unregistering hotkey ID associated with the calling thread.
        let raw_result = unsafe { raw_unregister_hotkey(HWND(std::ptr::null_mut()), id) };
        win32_result(raw_result, "UnregisterHotKey")?;

        self.registered_ids.retain(|&stored_id| stored_id != id);
        Ok(())
    }
}

impl Drop for WindowsGlobalHotkey {
    fn drop(&mut self) {
        for &id in &self.registered_ids {
            // SAFETY: Clean up hotkey registrations during drop without panicking.
            unsafe {
                let _ = raw_unregister_hotkey(HWND(std::ptr::null_mut()), id);
            }
        }
    }
}

// ============================================================================
// Windows Window Helper Implementation
// ============================================================================

/// Windows window helper implementing borderless styling, monitor centering, and z-ordering.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsWindowHelper {
    hwnd: Option<Hwnd>,
}

impl WindowsWindowHelper {
    /// Creates a new [`WindowsWindowHelper`].
    #[must_use]
    pub const fn new() -> Self {
        Self { hwnd: None }
    }

    /// Creates a new [`WindowsWindowHelper`] bound to a specific native window handle.
    #[must_use]
    pub const fn with_hwnd(hwnd: Hwnd) -> Self {
        Self { hwnd: Some(hwnd) }
    }

    fn get_hwnd(&self) -> Result<HWND> {
        let raw = self.hwnd.ok_or_else(|| {
            AppError::platform_failed("No active window handle assigned to WindowHelper")
        })?;
        Ok(HWND(raw as *mut std::ffi::c_void))
    }
}

impl WindowHelper for WindowsWindowHelper {
    #[allow(clippy::cast_possible_wrap)]
    fn set_borderless(&mut self, hwnd: Hwnd) -> Result<()> {
        self.hwnd = Some(hwnd);
        let win_hwnd = HWND(hwnd as *mut std::ffi::c_void);

        // 1. Fetch current extended window style
        // SAFETY: Clear last error before querying window attributes.
        unsafe { raw_set_last_error(WIN32_ERROR(0)) };
        let mut ex_style = unsafe { raw_get_window_long_ptr(win_hwnd, GWL_EXSTYLE) };
        let err = unsafe { raw_get_last_error() };
        if ex_style == 0 && err.0 != 0 {
            return Err(AppError::platform_failed(format!(
                "GetWindowLongPtrW(GWL_EXSTYLE) failed: Win32 error code {:#06X}",
                err.0
            )));
        }

        // Apply WS_EX_LAYERED | WS_EX_TOOLWINDOW as specified in Architectural Directive 2
        ex_style |= (WS_EX_LAYERED.0 | WS_EX_TOOLWINDOW.0) as isize;

        // SAFETY: Clear last error before setting extended window attributes.
        unsafe { raw_set_last_error(WIN32_ERROR(0)) };
        let prev_ex = unsafe { raw_set_window_long_ptr(win_hwnd, GWL_EXSTYLE, ex_style) };
        let err = unsafe { raw_get_last_error() };
        if prev_ex == 0 && err.0 != 0 {
            return Err(AppError::platform_failed(format!(
                "SetWindowLongPtrW(GWL_EXSTYLE) failed: Win32 error code {:#06X}",
                err.0
            )));
        }

        // 2. Fetch current standard window style and strip window chrome
        // SAFETY: Clear last error before querying window style attributes.
        unsafe { raw_set_last_error(WIN32_ERROR(0)) };
        let mut style = unsafe { raw_get_window_long_ptr(win_hwnd, GWL_STYLE) };
        let err = unsafe { raw_get_last_error() };
        if style == 0 && err.0 != 0 {
            return Err(AppError::platform_failed(format!(
                "GetWindowLongPtrW(GWL_STYLE) failed: Win32 error code {:#06X}",
                err.0
            )));
        }

        let border_flags =
            (WS_CAPTION.0 | WS_THICKFRAME.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0 | WS_SYSMENU.0)
                as isize;
        style &= !border_flags;
        style |= WS_POPUP.0 as isize;

        // SAFETY: Clear last error before setting standard window attributes.
        unsafe { raw_set_last_error(WIN32_ERROR(0)) };
        let prev_style = unsafe { raw_set_window_long_ptr(win_hwnd, GWL_STYLE, style) };
        let err = unsafe { raw_get_last_error() };
        if prev_style == 0 && err.0 != 0 {
            return Err(AppError::platform_failed(format!(
                "SetWindowLongPtrW(GWL_STYLE) failed: Win32 error code {:#06X}",
                err.0
            )));
        }

        // 3. Force non-client recalculation with SWP_FRAMECHANGED
        // SAFETY: Repositioning with SWP_FRAMECHANGED forces standard window chrome removal.
        let raw_result = unsafe {
            raw_set_window_pos(
                win_hwnd,
                HWND(std::ptr::null_mut()),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
            )
        };
        win32_result(raw_result, "SetWindowPos(SWP_FRAMECHANGED)")?;

        Ok(())
    }

    fn center_on_primary_monitor(&self) -> Result<()> {
        let hwnd = self.get_hwnd()?;

        // Retrieve primary monitor handle
        // SAFETY: MonitorFromWindow with MONITOR_DEFAULTTOPRIMARY is safe for any window.
        let hmonitor = unsafe { raw_monitor_from_window(hwnd, MONITOR_DEFAULTTOPRIMARY) };
        if hmonitor.is_invalid() {
            return Err(AppError::platform_failed(
                "MonitorFromWindow returned invalid monitor handle",
            ));
        }

        let cb_size = u32::try_from(size_of::<MONITORINFO>()).map_err(|err| {
            AppError::platform_failed(format!("MONITORINFO size conversion failed: {err}"))
        })?;

        let mut monitor_info = MONITORINFO {
            cbSize: cb_size,
            ..Default::default()
        };

        // SAFETY: Passing a valid pointer to initialized MONITORINFO struct.
        let bool_result = unsafe { raw_get_monitor_info(hmonitor, &raw mut monitor_info) };
        win32_result(bool_result, "GetMonitorInfoW")?;

        let mut window_rect = RECT::default();
        // SAFETY: Passing a valid pointer to RECT struct.
        let raw_result = unsafe { raw_get_window_rect(hwnd, &raw mut window_rect) };
        win32_result(raw_result, "GetWindowRect")?;

        let win_width = window_rect.right - window_rect.left;
        let win_height = window_rect.bottom - window_rect.top;
        let work_width = monitor_info.rcWork.right - monitor_info.rcWork.left;
        let work_height = monitor_info.rcWork.bottom - monitor_info.rcWork.top;

        let center_x = monitor_info.rcWork.left + (work_width - win_width) / 2;
        let center_y = monitor_info.rcWork.top + (work_height - win_height) / 2;

        // Reposition window to centered coordinates
        // SAFETY: Repositioning window within desktop monitor bounds.
        let raw_result = unsafe {
            raw_set_window_pos(
                hwnd,
                HWND(std::ptr::null_mut()),
                center_x,
                center_y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        win32_result(raw_result, "SetWindowPos(center)")?;

        Ok(())
    }

    fn set_topmost(&self, topmost: bool) -> Result<()> {
        let hwnd = self.get_hwnd()?;
        let insert_after = if topmost {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };

        // SAFETY: Updating topmost z-order position.
        let raw_result = unsafe {
            raw_set_window_pos(
                hwnd,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
        win32_result(raw_result, "SetWindowPos(HWND_TOPMOST)")?;

        Ok(())
    }

    fn focus(&self) -> Result<()> {
        let hwnd = self.get_hwnd()?;

        // SAFETY: SetFocus assigns keyboard focus to the target window.
        let raw_result = unsafe { raw_set_focus(hwnd) };
        win32_result(raw_result, "SetFocus")?;
        Ok(())
    }
}

// ============================================================================
// Windows Foreground Manager Implementation
// ============================================================================

/// Windows foreground manager applying the Alt-key synthesis technique to bypass
/// Windows focus stealing prevention.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsForegroundManager;

impl WindowsForegroundManager {
    /// Creates a new [`WindowsForegroundManager`].
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ForegroundManager for WindowsForegroundManager {
    fn bring_to_front(&self, hwnd: Hwnd) -> Result<()> {
        let target_hwnd = HWND(hwnd as *mut std::ffi::c_void);

        // Architectural Directive 2:
        // Use the standard Alt-key press trick to bypass Windows foreground-lock restrictions.
        // Simulating an Alt key down and up generates synthetic user input, which prompts
        // the Windows window manager to allow immediate SetForegroundWindow transitions.
        // SAFETY: Emitting extended Alt key down and up events.
        let vk_menu = u8::try_from(VK_MENU.0).unwrap_or(0x12);
        unsafe {
            raw_keybd_event(vk_menu, 0, KEYEVENTF_EXTENDEDKEY, 0);
            raw_keybd_event(vk_menu, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP, 0);
        }

        // SAFETY: BringWindowToTop brings target window above sibling windows.
        let top_result = unsafe { raw_bring_window_to_top(target_hwnd) };
        win32_result(top_result, "BringWindowToTop")?;

        // SAFETY: SetForegroundWindow activates and brings window to the foreground.
        let fg_result = unsafe { raw_set_foreground_window(target_hwnd) };
        win32_result(fg_result, "SetForegroundWindow")?;

        // SAFETY: SetFocus assigns keyboard focus to the newly activated window.
        let focus_result = unsafe { raw_set_focus(target_hwnd) };
        let _ = win32_result(focus_result, "SetFocus");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_win32_modifiers() {
        let mods = HotkeyModifiers {
            alt: true,
            ctrl: false,
            shift: false,
            win: false,
        };
        let win_mods = to_win32_modifiers(mods);
        assert_eq!(win_mods.0 & MOD_ALT.0, MOD_ALT.0);
        assert_eq!(win_mods.0 & MOD_NOREPEAT.0, MOD_NOREPEAT.0);
    }

    #[test]
    fn test_to_win32_vk() {
        assert_eq!(to_win32_vk(KeyCode::Space), 0x20);
        assert_eq!(to_win32_vk(KeyCode::Return), 0x0D);
        assert_eq!(to_win32_vk(KeyCode::Escape), 0x1B);
        assert_eq!(to_win32_vk(KeyCode::Char('a')), 0x41);
        assert_eq!(to_win32_vk(KeyCode::Char('Z')), 0x5A);
        assert_eq!(to_win32_vk(KeyCode::F(1)), u32::from(VK_F1.0));
        assert_eq!(to_win32_vk(KeyCode::F(12)), u32::from(VK_F1.0) + 11);
    }

    #[test]
    fn test_win32_result_success_and_failure() {
        assert!(win32_result(BOOL(1), "TestSuccess").is_ok());
        let failure = win32_result(BOOL(0), "TestFailure");
        assert!(failure.is_err());

        assert!(win32_result(Ok(()), "TestOk").is_ok());
    }

    #[test]
    fn test_window_helper_mock_hwnd() {
        // Documented limitation: Mock HWNDs without an active desktop window
        // cannot execute live Win32 repositioning, but constructor and state binding work.
        let mock_hwnd = 0x1234_5678 as Hwnd;
        let helper = WindowsWindowHelper::with_hwnd(mock_hwnd);
        assert!(helper.hwnd.is_some());
    }
}
