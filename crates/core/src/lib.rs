//! # Atalho Core Domain
//!
//! Foundational domain models, contracts, and error definitions for the Atalho desktop
//! application launcher.
//!
//! Following Clean Architecture principles, this crate encapsulates pure domain logic
//! and defines abstract traits for dependency inversion without coupling to concrete
//! I/O, UI frameworks, or platform-specific libraries.

mod error;
mod models;
mod traits;

// Re-export standard library core modules so macros (like `async_trait`) that emit `core::...`
// resolve correctly without shadowing conflicts.
#[doc(hidden)]
pub use ::core::{
    clone, cmp, convert, default, fmt, future, hash, hint, iter, marker, mem, ops, option, pin,
    ptr, result, slice, str, task,
};

pub use error::{AppError, Result};
pub use models::{AppEntry, CommandEntry, Score, SearchItem, SearchResult};
pub use traits::{Executor, Indexer, Matcher, Persistence};
