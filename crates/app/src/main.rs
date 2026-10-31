//! # Atalho Application Entry Point
//!
//! Composition root that initializes workspace components, loads indexed applications,
//! and runs the desktop launcher interface.

pub mod search;

use core::{AppEntry, AppSource, CommandEntry, Matcher, Persistence, SearchRequest};
use executor::SystemExecutor;
use indexer::DefaultIndexer;
use matcher::FuzzyMatcher;
use persistence::InMemoryPersistence;
use platform::PlatformManager;
use search::SearchService;
use std::sync::Arc;
use ui::LauncherState;

#[tokio::main]
async fn main() -> core::Result<()> {
    println!("Starting Atalho Application Launcher...");

    let platform = PlatformManager::new();
    println!("Platform detected: {:?}", platform.platform_kind());

    let sample_apps = vec![
        AppEntry::new("Calculator", "calc.exe", AppSource::StartMenu)
            .with_description("Perform mathematical calculations")
            .with_keywords(["math", "calc"]),
        AppEntry::new("Notepad", "notepad.exe", AppSource::StartMenu)
            .with_description("Simple plain-text editor")
            .with_keywords(["editor", "text"]),
    ];

    let sample_commands = vec![
        CommandEntry::new("lock", "Lock Workstation", "rundll32.exe")
            .with_args(["user32.dll,LockWorkStation"])
            .with_description("Locks the current user session")
            .with_keywords(["lock", "screen"]),
    ];

    let indexer = Arc::new(DefaultIndexer::with_entries(
        sample_apps.clone(),
        sample_commands,
    ));
    let matcher = Arc::new(FuzzyMatcher::new());
    let _executor = SystemExecutor::with_platform(platform);
    let persistence = Arc::new(InMemoryPersistence::new());
    persistence.save_apps(&sample_apps).await?;
    let search_service = SearchService::new(
        indexer,
        Arc::clone(&matcher) as Arc<dyn Matcher>,
        persistence,
    );
    search_service.initialize().await?;
    let mut state = LauncherState::new();

    // Demonstrate composition and matching through the search service
    let response = search_service
        .search(SearchRequest::new("Notepad", sample_apps.len()))
        .await?;
    println!(
        "Initial search indexing: {} items matched query",
        response.total
    );
    let search_results: Vec<core::SearchResult> = response
        .results
        .into_iter()
        .map(|m| {
            let score_val = u32::try_from(m.score.max(0)).unwrap_or(0);
            core::SearchResult::from_app(m.entry, core::Score::new(score_val))
                .with_match_indices(m.match_positions)
        })
        .collect();
    state.set_results(search_results);
    if let Some(selected) = state.selected_result() {
        println!(
            "Top match: {} (score: {})",
            selected.title,
            selected.score.value()
        );
    }

    let _ = search_service;
    println!("Atalho bootstrap initialized successfully.");
    Ok(())
}
