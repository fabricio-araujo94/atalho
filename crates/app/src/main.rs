//! # Atalho Application Entry Point
//!
//! Composition root that initializes workspace components, loads indexed applications,
//! and runs the desktop launcher interface.

use core::{AppEntry, CommandEntry, Matcher};
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
        AppEntry::new("calculator", "Calculator", "calc.exe")
            .with_description("Perform mathematical calculations")
            .with_keywords(["math", "calc"]),
        AppEntry::new("notepad", "Notepad", "notepad.exe")
            .with_description("Simple plain-text editor")
            .with_keywords(["editor", "text"]),
    ];

    let sample_commands = vec![
        CommandEntry::new("lock", "Lock Workstation", "rundll32.exe")
            .with_args(["user32.dll,LockWorkStation"])
            .with_description("Locks the current user session")
            .with_keywords(["lock", "screen"]),
    ];

    let indexer = DefaultIndexer::with_entries(sample_apps.clone(), sample_commands.clone());
    let matcher = FuzzyMatcher::new();
    let _executor = SystemExecutor::with_platform(platform);
    let _persistence = InMemoryPersistence::new();
    let mut state = LauncherState::new();

    // Demonstrate composition and matching
    match matcher.search("Notepad", &sample_apps, &sample_commands) {
        Ok(results) => {
            println!("Initial search indexing: {} items matched query", results.len());
            state.set_results(results);
            if let Some(selected) = state.selected_result() {
                println!("Top match: {} (score: {})", selected.title, selected.score.value());
            }
        }
        Err(err) => {
            eprintln!("Search matching failed: {err}");
        }
    }

    let _ = indexer;
    println!("Atalho bootstrap initialized successfully.");
}
