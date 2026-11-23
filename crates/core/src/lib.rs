//! # Atalho Core Domain
//!
//! Foundational domain models, contracts, and error definitions for the Atalho desktop
//! application launcher.
//!
//! Following Clean Architecture principles, this crate encapsulates pure domain logic
//! and defines abstract traits for dependency inversion without coupling to concrete
//! I/O, UI frameworks, or platform-specific libraries.

mod config;
mod error;
mod icon;
pub mod indexer;
pub mod matcher;
mod models;
pub mod platform;
mod search_event;
mod traits;

pub use config::{AppConfig, SourceToggle, ThemeName};
pub use error::{AppError, Result};
pub use icon::Image;
pub use indexer::Indexer;
pub use matcher::{Matcher, ScoredMatch, SearchRequest, SearchResponse};
pub use models::{
    AppEntry, AppSource, CommandEntry, Score, SearchItem, SearchResult, normalize_name,
};
pub use platform::{
    ForegroundManager, GlobalHotkey, HotkeyId, HotkeyModifiers, Hwnd, Key, KeyCode, Mods,
    WindowHelper,
};
pub use search_event::SearchEvent;
pub use traits::{CacheMetadata, Config, Executor, History, IndexCache, Persistence};
