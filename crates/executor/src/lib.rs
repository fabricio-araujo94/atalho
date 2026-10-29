//! Application and command execution module for Atalho.
//!
//! Provides concrete implementations of [`core::Executor`] leveraging [`platform::PlatformManager`]
//! to dispatch and launch desktop applications and system commands.

use core::{AppEntry, AppError, CommandEntry, Executor, Result};
use platform::PlatformManager;
use std::path::Path;

/// Default executor dispatching processes through [`PlatformManager`].
#[derive(Debug, Default, Clone)]
pub struct SystemExecutor {
    platform: PlatformManager,
}

impl SystemExecutor {
    /// Creates a new [`SystemExecutor`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            platform: PlatformManager::new(),
        }
    }

    /// Creates a new [`SystemExecutor`] using the given [`PlatformManager`].
    #[must_use]
    pub const fn with_platform(platform: PlatformManager) -> Self {
        Self { platform }
    }
}

#[async_trait::async_trait]
impl Executor for SystemExecutor {
    async fn execute_app(&self, app: &AppEntry) -> Result<()> {
        self.platform
            .launch_executable(&app.exec_path, &[])
            .map_err(|err| {
                AppError::execution_failed(format!(
                    "Failed to execute application '{}': {err}",
                    app.name
                ))
            })
    }

    async fn execute_command(&self, command: &CommandEntry) -> Result<()> {
        let path = Path::new(&command.command);
        self.platform
            .launch_executable(path, &command.args)
            .map_err(|err| {
                AppError::execution_failed(format!(
                    "Failed to execute command '{}': {err}",
                    command.title
                ))
            })
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
