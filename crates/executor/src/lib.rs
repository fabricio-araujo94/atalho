//! Asynchronous application execution with platform-specific process launching.

use core::{AppEntry, AppError, CommandEntry, Executor, History, Result};
use platform::PlatformManager;
#[cfg(windows)]
use std::path::Path;
use std::sync::Arc;

/// Default executor for the current platform.
pub struct SystemExecutor {
    platform: PlatformManager,
    history: Option<Arc<dyn History>>,
}

impl std::fmt::Debug for SystemExecutor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SystemExecutor")
            .field("platform", &self.platform)
            .field("records_history", &self.history.is_some())
            .finish()
    }
}

impl SystemExecutor {
    /// Creates a new executor for the current platform.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            platform: PlatformManager::new(),
            history: None,
        }
    }

    /// Creates an executor using the supplied platform facade.
    #[must_use]
    pub const fn with_platform(platform: PlatformManager) -> Self {
        Self {
            platform,
            history: None,
        }
    }

    /// Enables fire-and-forget launch history recording.
    #[must_use]
    pub fn with_history(mut self, history: Arc<dyn History>) -> Self {
        self.history = Some(history);
        self
    }

    async fn launch_entry(&self, app: &AppEntry) -> Result<()> {
        let application = app.clone();
        tokio::task::spawn_blocking(move || launch_application(&application))
            .await
            .map_err(|error| {
                AppError::execution_failed(format!("Application launcher task failed: {error}"))
            })??;

        if let Some(history) = &self.history {
            let history = Arc::clone(history);
            let entry_id = app.id;
            tokio::spawn(async move {
                if let Err(error) = history.record(entry_id).await {
                    tracing::warn!("Failed to record application launch: {error}");
                }
            });
        }
        Ok(())
    }
}

impl Default for SystemExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Windows-specific executor. On other platforms it uses the standard system executor.
pub struct WindowsExecutor(SystemExecutor);

impl WindowsExecutor {
    /// Creates a Windows executor with optional launch history.
    #[must_use]
    pub fn new(history: Option<Arc<dyn History>>) -> Self {
        let executor = SystemExecutor::new();
        Self(match history {
            Some(history) => executor.with_history(history),
            None => executor,
        })
    }
}

#[async_trait::async_trait]
impl Executor for SystemExecutor {
    async fn launch(&self, app: &AppEntry) -> Result<()> {
        self.launch_entry(app).await
    }

    async fn execute_command(&self, command: &CommandEntry) -> Result<()> {
        let command = command.clone();
        tokio::task::spawn_blocking(move || launch_command(&command))
            .await
            .map_err(|error| {
                AppError::execution_failed(format!("Command launcher task failed: {error}"))
            })?
    }
}

#[async_trait::async_trait]
impl Executor for WindowsExecutor {
    async fn launch(&self, app: &AppEntry) -> Result<()> {
        self.0.launch_entry(app).await
    }

    async fn execute_command(&self, command: &CommandEntry) -> Result<()> {
        Executor::execute_command(&self.0, command).await
    }
}

#[cfg(windows)]
fn launch_application(app: &AppEntry) -> Result<()> {
    if let Some(shortcut) = app.shortcut_path.as_ref() {
        return windows_process::shell_execute(shortcut, None, app.requires_admin)
            .map_err(|error| launch_error(app, error));
    }
    if app.requires_admin {
        return windows_process::shell_execute(
            &app.executable_path,
            app.arguments.as_deref(),
            true,
        )
        .map_err(|error| launch_error(app, error));
    }
    windows_process::create_process(&app.executable_path, app.arguments.as_deref())
        .map_err(|error| launch_error(app, error))
}

#[cfg(not(windows))]
fn launch_application(app: &AppEntry) -> Result<()> {
    let mut command = std::process::Command::new(&app.executable_path);
    if let Some(arguments) = &app.arguments {
        command.args(split_arguments(arguments));
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| launch_error(app, error))
}

#[cfg(windows)]
fn launch_command(command: &CommandEntry) -> Result<()> {
    let path = Path::new(&command.command);
    windows_process::create_process(path, Some(&command.args.join(" "))).map_err(|error| {
        AppError::execution_failed(format!(
            "Failed to execute command '{}': {error}",
            command.title
        ))
    })
}

#[cfg(not(windows))]
fn launch_command(command: &CommandEntry) -> Result<()> {
    std::process::Command::new(&command.command)
        .args(&command.args)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            AppError::execution_failed(format!(
                "Failed to execute command '{}': {error}",
                command.title
            ))
        })
}

fn launch_error(app: &AppEntry, error: impl std::fmt::Display) -> AppError {
    AppError::execution_failed(format!(
        "Failed to execute application '{}': {error}",
        app.name
    ))
}

#[cfg(not(windows))]
fn split_arguments(arguments: &str) -> Vec<&str> {
    arguments.split_whitespace().collect()
}

#[cfg(windows)]
mod windows_process {
    use std::ffi::OsStr;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
    use windows::Win32::System::Threading::{
        CreateProcessW, DETACHED_PROCESS, PROCESS_INFORMATION, STARTUPINFOW,
    };
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{PCWSTR, PWSTR};

    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                // SAFETY: Owns the process/thread handle returned by CreateProcessW.
                unsafe {
                    let _ = CloseHandle(self.0);
                }
            }
        }
    }

    pub fn shell_execute(
        path: &Path,
        arguments: Option<&str>,
        requires_admin: bool,
    ) -> std::result::Result<(), String> {
        let path_wide = wide(path.as_os_str());
        let args_wide = arguments.map(|arguments| wide(OsStr::new(arguments)));
        let verb = if requires_admin {
            wide(OsStr::new("runas"))
        } else {
            wide(OsStr::new("open"))
        };
        // SAFETY: All pointers refer to NUL-terminated buffers that remain live for the call.
        let result = unsafe {
            ShellExecuteW(
                HWND::default(),
                PCWSTR(verb.as_ptr()),
                PCWSTR(path_wide.as_ptr()),
                args_wide
                    .as_ref()
                    .map_or(PCWSTR::null(), |arguments| PCWSTR(arguments.as_ptr())),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        let error_code = result.0 as usize;
        if error_code <= 32 {
            let description = match error_code {
                0 => "out of memory or resources",
                2 => "file not found",
                3 => "path not found",
                5 => "access denied",
                8 => "out of memory",
                26 => "sharing violation",
                27 => "incomplete file association",
                28 => "DDE timeout",
                29 => "DDE transaction failed",
                30 => "DDE is busy",
                31 => "no application is associated with this file type",
                32 => "required DLL not found",
                _ => "unknown shell execution error",
            };
            Err(format!(
                "ShellExecuteW returned {error_code}: {description}"
            ))
        } else {
            Ok(())
        }
    }

    pub fn create_process(path: &Path, arguments: Option<&str>) -> windows::core::Result<()> {
        let path_wide = wide(path.as_os_str());
        let is_batch = path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            });
        let (application_name, command_line) = if is_batch {
            let command_interpreter =
                std::env::var_os("COMSPEC").unwrap_or_else(|| OsStr::new("cmd.exe").to_os_string());
            let application_name = Path::new(&command_interpreter)
                .is_absolute()
                .then(|| wide(&command_interpreter));
            let command = format!(
                "\"{}\" /D /S /C \"\"{}\" {}\"",
                command_interpreter.to_string_lossy(),
                path.display(),
                arguments.unwrap_or_default()
            );
            (application_name, wide(OsStr::new(&command)))
        } else {
            let command = match arguments {
                Some(arguments) if !arguments.is_empty() => {
                    format!("\"{}\" {arguments}", path.display())
                }
                _ => format!("\"{}\"", path.display()),
            };
            (
                path.is_absolute().then_some(path_wide),
                wide(OsStr::new(&command)),
            )
        };

        let startup_info = STARTUPINFOW {
            cb: u32::try_from(size_of::<STARTUPINFOW>()).unwrap_or(u32::MAX),
            ..STARTUPINFOW::default()
        };
        let mut process_info = PROCESS_INFORMATION::default();
        let current_directory = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| wide(parent.as_os_str()));
        // SAFETY: `command_line` is mutable, NUL-terminated and remains alive for this call.
        unsafe {
            CreateProcessW(
                application_name
                    .as_ref()
                    .map_or(PCWSTR::null(), |name| PCWSTR(name.as_ptr())),
                PWSTR(command_line.as_ptr().cast_mut()),
                None,
                None,
                false,
                DETACHED_PROCESS,
                None,
                current_directory
                    .as_ref()
                    .map_or(PCWSTR::null(), |directory| PCWSTR(directory.as_ptr())),
                &raw const startup_info,
                &raw mut process_info,
            )?;
        }
        let _process = OwnedHandle(process_info.hProcess);
        let _thread = OwnedHandle(process_info.hThread);
        Ok(())
    }

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain([0]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_executor_constructors() {
        let executor = SystemExecutor::new();
        assert_eq!(
            executor.platform.platform_kind(),
            PlatformManager::new().platform_kind()
        );
    }
}
