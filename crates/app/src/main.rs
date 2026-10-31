//! # Atalho Application Entry Point
//!
//! Composition root that initializes workspace components, loads indexed applications,
//! and runs the desktop launcher interface.

use core::{AppEntry, AppSource, CommandEntry, Matcher};
use executor::SystemExecutor;
use indexer::DefaultIndexer;
use matcher::FuzzyMatcher;
use persistence::InMemoryPersistence;
use platform::PlatformManager;
use ui::LauncherState;

fn main() {
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

    let indexer = DefaultIndexer::with_entries(sample_apps.clone(), sample_commands);
    let matcher = FuzzyMatcher::new();
    let _executor = SystemExecutor::with_platform(platform);
    let _persistence = InMemoryPersistence::new();
    let mut state = LauncherState::new();

    // Demonstrate composition and matching
    let results = matcher.search("Notepad", &sample_apps);
    println!(
        "Initial search indexing: {} items matched query",
        results.len()
    );
    let search_results: Vec<core::SearchResult> = results
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

    let _ = indexer;
    println!("Atalho bootstrap initialized successfully.");
}
