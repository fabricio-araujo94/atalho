//! Background-utility lifecycle facilities: tray events, single-instance signaling,
//! and per-user sign-in startup registration.

use core::{AppError, Result};
use std::sync::mpsc;

/// Actions originating from the system tray or a second launcher instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Toggle launcher visibility.
    ToggleVisibility,
    /// Show the launcher window.
    Show,
    /// Open launcher settings.
    Settings,
    /// Request orderly shutdown.
    Quit,
}

#[cfg(windows)]
mod windows_impl {
    use super::{AppError, Result, TrayEvent, mpsc};
    use std::ffi::OsStr;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Sender, SyncSender};
    use std::thread::{self, JoinHandle};
    use std::time::Instant;
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, POINT, WPARAM,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile,
    };
    use windows::Win32::System::Threading::{
        CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject,
    };
    use windows::Win32::UI::Shell::{
        IShellLinkW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFY_ICON_DATA_FLAGS,
        NOTIFYICONDATAW, Shell_NotifyIconW, ShellLink,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CREATESTRUCTW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetMessageW,
        GetWindowLongPtrW, IDI_APPLICATION, LoadIconW, MF_STRING, MSG, PostMessageW,
        PostThreadMessageW, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, TPM_NONOTIFY,
        TPM_RETURNCMD, TrackPopupMenu, TranslateMessage, WNDCLASSW, WS_OVERLAPPED,
    };
    use windows::core::{Interface, PCWSTR};

    const MUTEX_NAME: &str = "Global\\Atalho.SingleInstance";
    const SHOW_EVENT_NAME: &str = "Global\\Atalho.Show";
    const TRAY_CLASS: &str = "AtalhoTrayWindow";
    const TRAY_MESSAGE: u32 = 0x8000 + 41;
    const WM_NCCREATE: u32 = 0x0081;
    const WM_NCDESTROY: u32 = 0x0082;
    const WM_CONTEXTMENU: u32 = 0x007B;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_QUIT: u32 = 0x0012;
    const MENU_SHOW: usize = 1;
    const MENU_SETTINGS: usize = 2;
    const MENU_QUIT: usize = 3;

    struct ComGuard;

    impl Drop for ComGuard {
        fn drop(&mut self) {
            // SAFETY: This guard is created only after successful COM initialization.
            unsafe { CoUninitialize() };
        }
    }

    /// Owns the named mutex and secondary-instance show-signal listener.
    pub struct SingleInstanceGuard {
        mutex: HANDLE,
        show_event: HANDLE,
        stop_listener: std::sync::Arc<AtomicBool>,
        listener: Option<JoinHandle<Result<()>>>,
    }

    impl SingleInstanceGuard {
        /// Returns `None` after signaling the existing launcher if another instance owns the mutex.
        ///
        /// # Errors
        /// Returns an error if the named synchronization objects cannot be created.
        pub fn acquire() -> Result<Option<Self>> {
            let event_name = wide(SHOW_EVENT_NAME);
            // Create the event before the mutex so a racing secondary process can always signal it.
            let show_event =
                unsafe { CreateEventW(None, false, false, PCWSTR(event_name.as_ptr())) }.map_err(
                    |error| {
                        AppError::platform_failed(format!("Create launcher show event: {error}"))
                    },
                )?;
            let mutex_name = wide(MUTEX_NAME);
            let mutex = unsafe { CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr())) }.map_err(
                |error| {
                    unsafe {
                        let _ = CloseHandle(show_event);
                    }
                    AppError::platform_failed(format!("Create single-instance mutex: {error}"))
                },
            )?;
            let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
            if already_exists {
                let _ = unsafe { SetEvent(show_event) };
                unsafe {
                    let _ = CloseHandle(mutex);
                    let _ = CloseHandle(show_event);
                }
                return Ok(None);
            }
            Ok(Some(Self {
                mutex,
                show_event,
                stop_listener: std::sync::Arc::new(AtomicBool::new(false)),
                listener: None,
            }))
        }

        /// Starts a dedicated listener that forwards secondary-instance requests.
        ///
        /// # Errors
        /// Returns an error if the signal-listener thread cannot be started.
        pub fn listen_for_show(&mut self, sender: Sender<TrayEvent>) -> Result<()> {
            let event = self.show_event.0 as usize;
            let stop = std::sync::Arc::clone(&self.stop_listener);
            self.listener = Some(
                thread::Builder::new()
                    .name("launcher-show-signal".to_owned())
                    .spawn(move || -> Result<()> {
                        while !stop.load(Ordering::Acquire) {
                            let result =
                                unsafe { WaitForSingleObject(HANDLE(event as *mut _), 250) };
                            if result.0 == u32::MAX {
                                return Err(AppError::platform_failed(
                                    "Wait for second-instance signal failed",
                                ));
                            }
                            if result.0 == 0 && sender.send(TrayEvent::Show).is_err() {
                                break;
                            }
                        }
                        Ok(())
                    })
                    .map_err(|error| {
                        AppError::platform_failed(format!("Start show-signal listener: {error}"))
                    })?,
            );
            Ok(())
        }

        /// Stops the second-instance notification listener while retaining the mutex.
        ///
        /// # Errors
        /// Returns an error if the listener cannot be awakened or joined.
        pub fn stop_listening(&mut self) -> Result<()> {
            self.stop_listener.store(true, Ordering::Release);
            unsafe { SetEvent(self.show_event) }.map_err(|error| {
                AppError::platform_failed(format!(
                    "Could not wake the single-instance signal listener: {error}"
                ))
            })?;
            if let Some(listener) = self.listener.take() {
                listener
                    .join()
                    .map_err(|_| AppError::platform_failed("Show-signal listener panicked"))??;
            }
            Ok(())
        }
    }

    impl Drop for SingleInstanceGuard {
        fn drop(&mut self) {
            let _ = self.stop_listening();
            unsafe {
                let _ = CloseHandle(self.show_event);
                let _ = CloseHandle(self.mutex);
            }
        }
    }

    /// Runs the tray window and Win32 message pump on its own thread.
    pub struct TrayController {
        thread_id: u32,
        thread: Option<JoinHandle<Result<()>>>,
    }

    impl TrayController {
        /// Creates the notification icon and returns its event receiver.
        ///
        /// # Errors
        /// Returns an error if the tray thread or notification icon cannot be initialized.
        pub fn start(sender: Sender<TrayEvent>) -> Result<Self> {
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            let thread = thread::Builder::new()
                .name("launcher-system-tray".to_owned())
                .spawn(move || tray_thread(sender, &ready_sender))
                .map_err(|error| {
                    AppError::platform_failed(format!("Start system tray thread: {error}"))
                })?;
            let readiness = match ready_receiver.recv() {
                Ok(readiness) => readiness,
                Err(error) => {
                    let _ = thread.join();
                    return Err(AppError::platform_failed(format!(
                        "Wait for system tray initialization: {error}"
                    )));
                }
            };
            match readiness {
                Ok(thread_id) => Ok(Self {
                    thread_id,
                    thread: Some(thread),
                }),
                Err(error) => {
                    let _ = thread.join();
                    Err(error)
                }
            }
        }

        /// Removes the tray icon and joins its message-pump thread.
        ///
        /// # Errors
        /// Returns an error if the tray thread cannot be stopped or cleanup fails.
        pub fn shutdown(&mut self) -> Result<()> {
            if let Some(thread) = self.thread.take() {
                let post_result = unsafe {
                    PostThreadMessageW(
                        self.thread_id,
                        WM_QUIT,
                        WPARAM::default(),
                        LPARAM::default(),
                    )
                };
                let join_result = thread.join().map_err(|_| {
                    AppError::platform_failed("System tray thread panicked during shutdown")
                })?;
                post_result.map_err(|error| {
                    AppError::platform_failed(format!("Stop system tray thread: {error}"))
                })?;
                join_result?;
            }
            Ok(())
        }
    }

    impl Drop for TrayController {
        fn drop(&mut self) {
            let _ = self.shutdown();
        }
    }

    fn tray_thread(sender: Sender<TrayEvent>, ready: &SyncSender<Result<u32>>) -> Result<()> {
        unsafe {
            let thread_id = windows::Win32::System::Threading::GetCurrentThreadId();
            let class_name = wide(TRAY_CLASS);
            let context = Box::new(TrayContext {
                sender,
                last_click: None,
            });
            let context_ptr = Box::into_raw(context);
            let class = WNDCLASSW {
                lpfnWndProc: Some(tray_window_proc),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..WNDCLASSW::default()
            };
            RegisterClassW(&raw const class);
            let window = CreateWindowExW(
                windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE::default(),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(class_name.as_ptr()),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                Some(context_ptr.cast()),
            );
            let window = match window {
                Ok(window) => window,
                Err(error) => {
                    drop(Box::from_raw(context_ptr));
                    let _ = ready.send(Err(AppError::platform_failed(format!(
                        "Create tray message window: {error}"
                    ))));
                    return Err(AppError::platform_failed("Tray window creation failed"));
                }
            };
            let mut data = NOTIFYICONDATAW {
                cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).unwrap_or(u32::MAX),
                hWnd: window,
                uID: 1,
                uFlags: NOTIFY_ICON_DATA_FLAGS(NIF_ICON.0 | NIF_MESSAGE.0 | NIF_TIP.0),
                uCallbackMessage: TRAY_MESSAGE,
                hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
                ..NOTIFYICONDATAW::default()
            };
            let tip = wide("Atalho");
            for (slot, unit) in data.szTip.iter_mut().zip(tip.iter().copied()) {
                *slot = unit;
            }
            if !Shell_NotifyIconW(NIM_ADD, &raw const data).as_bool() {
                let _ = DestroyWindow(window);
                let _ = ready.send(Err(AppError::platform_failed(
                    "Shell_NotifyIconW could not add the launcher tray icon",
                )));
                return Err(AppError::platform_failed(
                    "Could not install the launcher tray icon",
                ));
            }
            let _ = ready.send(Ok(thread_id));
            let mut message = MSG::default();
            while GetMessageW(&raw mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
            let removed = Shell_NotifyIconW(NIM_DELETE, &raw const data).as_bool();
            let destroyed = DestroyWindow(window);
            if !removed {
                return Err(AppError::platform_failed(
                    "Shell_NotifyIconW could not remove the launcher tray icon",
                ));
            }
            destroyed.map_err(|error| {
                AppError::platform_failed(format!("Destroy tray message window: {error}"))
            })?;
        }
        Ok(())
    }

    struct TrayContext {
        sender: Sender<TrayEvent>,
        last_click: Option<Instant>,
    }

    unsafe extern "system" fn tray_window_proc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        if message == WM_NCCREATE {
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            let _ =
                unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize) };
            return unsafe { DefWindowProcW(window, message, wparam, lparam) };
        }
        let context_ptr = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut TrayContext;
        if message == WM_NCDESTROY {
            if !context_ptr.is_null() {
                unsafe { drop(Box::from_raw(context_ptr)) };
            }
            return unsafe { DefWindowProcW(window, message, wparam, lparam) };
        }
        if !context_ptr.is_null() {
            let context = unsafe { &mut *context_ptr };
            if message == TRAY_MESSAGE {
                match u32::try_from(lparam.0 & 0xFFFF).unwrap_or_default() {
                    WM_LBUTTONUP => {
                        let now = Instant::now();
                        if context
                            .last_click
                            .is_none_or(|last| now.duration_since(last).as_millis() > 500)
                        {
                            context.last_click = Some(now);
                            let _ = context.sender.send(TrayEvent::ToggleVisibility);
                        }
                    }
                    WM_RBUTTONUP | WM_CONTEXTMENU => show_tray_menu(window),
                    _ => {}
                }
                return windows::Win32::Foundation::LRESULT(0);
            }
        }
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    fn show_tray_menu(window: HWND) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return;
            };
            let labels = [
                (MENU_SHOW, "Show"),
                (MENU_SETTINGS, "Settings"),
                (MENU_QUIT, "Quit"),
            ];
            for (id, label) in labels {
                let label = wide(label);
                let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(label.as_ptr()));
            }
            let mut point = POINT::default();
            let _ = GetCursorPos(&raw mut point);
            let _ = SetForegroundWindow(window);
            let selection = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY,
                point.x,
                point.y,
                0,
                window,
                None,
            );
            let _ = PostMessageW(window, 0, WPARAM::default(), LPARAM::default());
            let _ = DestroyMenu(menu);
            let event = match usize::try_from(selection.0).unwrap_or_default() {
                MENU_SHOW => Some(TrayEvent::Show),
                MENU_SETTINGS => Some(TrayEvent::Settings),
                MENU_QUIT => Some(TrayEvent::Quit),
                _ => None,
            };
            if let Some(event) = event {
                let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut TrayContext;
                if !pointer.is_null() {
                    let _ = (*pointer).sender.send(event);
                }
            }
        }
    }

    /// Adds or removes the current executable from the current user's Startup folder.
    ///
    /// # Errors
    /// Returns an error if the Startup shortcut cannot be written or removed.
    pub fn set_auto_start(enabled: bool) -> Result<()> {
        let startup = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| AppError::platform_failed("APPDATA is not set"))?
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
            .join("Atalho.lnk");
        if !enabled {
            return match std::fs::remove_file(&startup) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(AppError::io_failed(format!(
                    "{}: remove auto-start shortcut: {error}",
                    startup.display()
                ))),
            };
        }
        std::fs::create_dir_all(startup.parent().ok_or_else(|| {
            AppError::io_failed(format!("{}: missing Startup directory", startup.display()))
        })?)
        .map_err(|error| {
            AppError::io_failed(format!(
                "{}: create Startup directory: {error}",
                startup.display()
            ))
        })?;
        let executable = std::env::current_exe().map_err(|error| {
            AppError::platform_failed(format!("Resolve launcher executable: {error}"))
        })?;
        let executable_wide = wide_os(executable.as_os_str());
        let working_directory = executable.parent().unwrap_or(&executable);
        let working_wide = wide_os(working_directory.as_os_str());
        let shortcut_wide = wide_os(startup.as_os_str());
        let com_result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok();
        com_result.map_err(|error| {
            AppError::platform_failed(format!("Initialize COM for auto-start shortcut: {error}"))
        })?;
        let _com_guard = ComGuard;
        let result = unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|error| {
                    AppError::platform_failed(format!("Create auto-start shell link: {error}"))
                })?;
            link.SetPath(PCWSTR(executable_wide.as_ptr()))
                .map_err(|error| {
                    AppError::platform_failed(format!("Set startup target: {error}"))
                })?;
            link.SetWorkingDirectory(PCWSTR(working_wide.as_ptr()))
                .map_err(|error| {
                    AppError::platform_failed(format!("Set startup working directory: {error}"))
                })?;
            let persist: IPersistFile = link.cast().map_err(|error| {
                AppError::platform_failed(format!("Access startup shortcut persistence: {error}"))
            })?;
            persist
                .Save(PCWSTR(shortcut_wide.as_ptr()), true)
                .map_err(|error| {
                    AppError::io_failed(format!(
                        "{}: save auto-start shortcut: {error}",
                        startup.display()
                    ))
                })
        };
        result
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain([0]).collect()
    }

    fn wide_os(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain([0]).collect()
    }
}

#[cfg(not(windows))]
mod windows_impl {
    use super::{AppError, Result, TrayEvent, mpsc};
    use std::sync::mpsc::Sender;

    pub struct SingleInstanceGuard;

    impl SingleInstanceGuard {
        /// Returns `None` after signaling the existing launcher if another instance owns the mutex.
        ///
        /// # Errors
        /// Returns an error if instance coordination cannot be initialized.
        pub fn acquire() -> Result<Option<Self>> {
            Ok(Some(Self))
        }

        /// Starts a no-op show listener on non-Windows systems.
        ///
        /// # Errors
        /// This implementation does not fail.
        pub fn listen_for_show(&mut self, _sender: mpsc::Sender<TrayEvent>) -> Result<()> {
            Ok(())
        }

        /// Stops the show listener.
        ///
        /// # Errors
        /// This implementation does not fail.
        pub fn stop_listening(&mut self) -> Result<()> {
            Ok(())
        }
    }

    pub struct TrayController;

    impl TrayController {
        /// Creates the non-Windows tray stub.
        ///
        /// # Errors
        /// This implementation does not fail.
        pub fn start(_sender: Sender<TrayEvent>) -> Result<Self> {
            Ok(Self)
        }

        /// Stops the tray stub.
        ///
        /// # Errors
        /// This implementation does not fail.
        pub fn shutdown(&mut self) -> Result<()> {
            Ok(())
        }
    }

    /// Reports that auto-start registration is unavailable on non-Windows platforms.
    ///
    /// # Errors
    /// Always returns an unsupported-platform error.
    pub fn set_auto_start(_enabled: bool) -> Result<()> {
        Err(AppError::platform_failed(
            "Automatic startup registration is only supported on Windows",
        ))
    }
}

pub use windows_impl::{SingleInstanceGuard, TrayController, set_auto_start};
