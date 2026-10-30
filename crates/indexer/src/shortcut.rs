//! Windows shell link (.lnk) parsing via COM and `IShellLinkW`.

#[cfg(windows)]
mod windows_impl {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, SLR_NO_UI, ShellLink};
    use windows::core::{Interface, PCWSTR};

    /// RAII guard ensuring `CoUninitialize` is called when leaving scope.
    pub struct ComGuard {
        _private: (),
    }

    impl ComGuard {
        /// Initializes the COM library for the calling thread with multi-threaded concurrency.
        ///
        /// # Errors
        /// Returns [`windows::core::Error`] if COM initialization fails.
        pub fn new() -> windows::core::Result<Self> {
            // SAFETY: Initializing COM library on the current thread.
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            }
            Ok(Self { _private: () })
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            // SAFETY: Balancing successful CoInitializeEx invocation.
            unsafe {
                CoUninitialize();
            }
        }
    }

    /// Extracted properties from a Windows shortcut (.lnk) file.
    #[derive(Debug, Clone, Default)]
    pub struct ShortcutInfo {
        /// Resolved target executable path.
        pub target_path: PathBuf,
        /// Optional command-line arguments.
        pub arguments: Option<String>,
        /// Optional icon location path.
        pub icon_path: Option<PathBuf>,
    }

    /// Parses a `.lnk` file from disk and extracts target path, arguments, and icon location.
    ///
    /// # Errors
    /// Returns [`windows::core::Error`] if loading or resolving the shortcut fails.
    pub fn parse_shortcut(lnk_path: &Path) -> windows::core::Result<ShortcutInfo> {
        let _guard = ComGuard::new()?;

        // SAFETY: Instantiating ShellLink COM class in-process.
        let shell_link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };

        let persist: IPersistFile = shell_link.cast()?;

        let mut wide_path: Vec<u16> = lnk_path.as_os_str().encode_wide().collect();
        wide_path.push(0);

        // SAFETY: Loading shortcut file into IPersistFile with read-only access.
        unsafe {
            persist.Load(PCWSTR(wide_path.as_ptr()), STGM_READ)?;
        }

        // SAFETY: Resolving link without displaying UI dialogs if broken.
        unsafe {
            let _ = shell_link.Resolve(HWND(std::ptr::null_mut()), SLR_NO_UI.0 as u32);
        }

        // 1. Target path
        let mut target_buf = [0u16; 1024];
        // SAFETY: Querying target path into preallocated buffer.
        unsafe {
            shell_link.GetPath(&mut target_buf, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)?;
        }
        let target_len = target_buf.iter().position(|&c| c == 0).unwrap_or(0);
        if target_len == 0 {
            return Err(windows::core::Error::from_win32());
        }
        let target_os = OsString::from_wide(&target_buf[..target_len]);
        let raw_target = PathBuf::from(target_os);
        let target_path = dunce::canonicalize(&raw_target).unwrap_or(raw_target);

        // 2. Arguments
        let mut args_buf = [0u16; 1024];
        // SAFETY: Querying arguments string.
        let _ = unsafe { shell_link.GetArguments(&mut args_buf) };
        let args_len = args_buf.iter().position(|&c| c == 0).unwrap_or(0);
        let arguments = if args_len > 0 {
            let s = String::from_utf16_lossy(&args_buf[..args_len])
                .trim()
                .to_string();
            if s.is_empty() { None } else { Some(s) }
        } else {
            None
        };

        // 3. Icon path
        let mut icon_buf = [0u16; 1024];
        let mut icon_index = 0i32;
        // SAFETY: Querying icon location string.
        let _ = unsafe { shell_link.GetIconLocation(&mut icon_buf, &raw mut icon_index) };
        let icon_len = icon_buf.iter().position(|&c| c == 0).unwrap_or(0);
        let icon_path = if icon_len > 0 {
            let icon_os = OsString::from_wide(&icon_buf[..icon_len]);
            let raw_icon = PathBuf::from(icon_os);
            if raw_icon.exists() {
                Some(dunce::canonicalize(&raw_icon).unwrap_or(raw_icon))
            } else {
                None
            }
        } else {
            None
        };

        Ok(ShortcutInfo {
            target_path,
            arguments,
            icon_path,
        })
    }
}

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(not(windows))]
mod stub_impl {
    use std::path::{Path, PathBuf};

    #[derive(Debug, Clone, Default)]
    pub struct ShortcutInfo {
        pub target_path: PathBuf,
        pub arguments: Option<String>,
        pub icon_path: Option<PathBuf>,
    }

    pub fn parse_shortcut(_lnk_path: &Path) -> Result<ShortcutInfo, ()> {
        Err(())
    }
}

#[cfg(not(windows))]
pub use stub_impl::*;
