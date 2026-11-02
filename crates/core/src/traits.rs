//! Core domain contracts defining the primary abstraction boundaries for Atalho.

use crate::config::AppConfig;
use crate::error::Result;
use crate::models::{AppEntry, CommandEntry, SearchItem, SearchResult};
use std::collections::HashMap;
use std::time::SystemTime;
use uuid::Uuid;

/// Metadata describing the last persisted application index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheMetadata {
    /// Time at which the cache was last written.
    pub timestamp: SystemTime,
    /// Number of application entries in the cache.
    pub count: usize,
}

/// Storage contract for the serialized application index.
#[async_trait::async_trait]
pub trait IndexCache: Send + Sync {
    /// Loads the application index, or returns `None` when no cache exists.
    async fn load(&self) -> Result<Option<Vec<AppEntry>>>;
    /// Atomically stores the application index.
    async fn save(&self, entries: &[AppEntry]) -> Result<()>;
    /// Returns metadata for the stored index.
    async fn metadata(&self) -> Result<CacheMetadata>;
}

/// Storage contract for application launch history.
#[async_trait::async_trait]
pub trait History: Send + Sync {
    /// Appends a launch event for an application.
    async fn record(&self, entry_id: Uuid) -> Result<()>;
    /// Returns application IDs from newest to oldest.
    async fn recent(&self, limit: usize) -> Result<Vec<Uuid>>;
    /// Returns launch counts keyed by application ID.
    async fn frequency(&self) -> Result<HashMap<Uuid, u32>>;
}

/// Storage contract for application configuration.
pub trait Config: Send + Sync {
    /// Loads configuration, returning defaults when no file exists.
    ///
    /// # Errors
    /// Returns an I/O error when the configuration file cannot be read or parsed.
    fn load(&self) -> Result<AppConfig>;
    /// Atomically saves configuration.
    ///
    /// # Errors
    /// Returns an I/O error when configuration cannot be serialized or written.
    fn save(&self, cfg: &AppConfig) -> Result<()>;
}

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

    /// Returns when the persisted application index was last refreshed.
    ///
    /// Implementations without freshness metadata may return `None`; callers should
    /// treat that cache as stale and rebuild it.
    async fn apps_last_updated(&self) -> Result<Option<SystemTime>> {
        Ok(None)
    }

    /// Asynchronously records the execution of an entry in launch history.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError::IoFailed`] if updating history fails.
    async fn record_launch(&self, entry_id: &str) -> Result<()>;
}
