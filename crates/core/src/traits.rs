//! Core domain contracts defining the primary abstraction boundaries for Atalho.

use crate::error::Result;
use crate::models::{AppEntry, CommandEntry, SearchItem, SearchResult};

/// Contract for launching desktop applications and executing system commands.
#[async_trait::async_trait]
pub trait Executor: Send + Sync {
    /// Asynchronously launches an indexed application.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::ExecutionFailed`] if the process cannot be spawned.
    async fn execute_app(&self, app: &AppEntry) -> Result<()>;

    /// Asynchronously executes a system command action.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::ExecutionFailed`] if the command cannot be executed.
    async fn execute_command(&self, command: &CommandEntry) -> Result<()>;

    /// Asynchronously executes a search result item by dispatching to either `execute_app` or `execute_command`.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::ExecutionFailed`] if dispatch or execution fails.
    async fn execute_result(&self, result: &SearchResult) -> Result<()> {
        match &result.item {
            SearchItem::App(app) => self.execute_app(app).await,
            SearchItem::Command(cmd) => self.execute_command(cmd).await,
        }
    }
}

/// Contract for persisting and retrieving application data, cache, and launch history.
#[async_trait::async_trait]
pub trait Persistence: Send + Sync {
    /// Asynchronously stores the list of indexed applications in persistent storage.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::IoFailed`] or [`crate::error::AppError::ExecutionFailed`] if persistence fails.
    async fn save_apps(&self, apps: &[AppEntry]) -> Result<()>;

    /// Asynchronously loads the list of indexed applications from persistent storage.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::IoFailed`] if loading or deserialization fails.
    async fn load_apps(&self) -> Result<Vec<AppEntry>>;

    /// Asynchronously records the execution of an entry in launch history.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::IoFailed`] if updating history fails.
    async fn record_launch(&self, entry_id: &str) -> Result<()>;
}
