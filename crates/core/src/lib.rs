//! # Atalho Core Domain
//!
//! Foundational domain models, contracts, and error definitions for the Atalho desktop
//! application launcher.
//!
//! Following Clean Architecture principles, this crate encapsulates pure domain logic
//! and defines abstract traits for dependency inversion without coupling to concrete
//! I/O, UI frameworks, or platform-specific libraries.

mod error;
pub mod indexer;
mod models;
pub mod platform;
mod traits;

// Re-export standard library core modules so macros (like `async_trait`) that emit `core::...`
// resolve correctly without shadowing conflicts.
#[doc(hidden)]
pub use ::core::{
    clone, cmp, convert, default, fmt, future, hash, hint, iter, marker, mem, ops, option, pin,
    prelude, ptr, result, slice, str, task,
};

pub use error::{AppError, Result};
pub use indexer::Indexer;
pub use models::{
    AppEntry, AppSource, CommandEntry, Score, SearchItem, SearchResult, normalize_name,
};
pub use platform::{
    ForegroundManager, GlobalHotkey, HotkeyId, HotkeyModifiers, Hwnd, Key, KeyCode, Mods,
    WindowHelper,
};
pub use traits::{Executor, Matcher, Persistence};
