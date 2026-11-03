//! # Atalho Application Entry Point
//!
//! Composition root that initializes workspace components, loads indexed applications,
//! and runs the desktop launcher interface.

pub mod search;

use core::{AppEntry, AppSource, CommandEntry, Executor, Matcher, Persistence};
use executor::SystemExecutor;
use indexer::DefaultIndexer;
use matcher::FuzzyMatcher;
use persistence::InMemoryPersistence;
use platform::PlatformManager;
use search::SearchService;
use std::sync::Arc;
use tokio::sync::mpsc;

fn main() -> core::Result<()> {
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
    let executor = Arc::new(SystemExecutor::with_platform(platform));
    let persistence = Arc::new(InMemoryPersistence::new());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            core::AppError::execution_failed(format!("Tokio startup failed: {error}"))
        })?;
    let search_service = Arc::new(SearchService::new(
        indexer,
        Arc::clone(&matcher) as Arc<dyn Matcher>,
        persistence.clone(),
    ));
    runtime.block_on(async {
        persistence.save_apps(&sample_apps).await?;
        search_service.initialize().await
    })?;

    let (query_sender, mut query_receiver) = mpsc::unbounded_channel();
    let service = Arc::clone(&search_service);
    runtime.spawn(async move {
        while let Some(request) = query_receiver.recv().await {
            service.submit_query(request);
        }
    });

    let (execute_sender, mut execute_receiver) = mpsc::unbounded_channel();
    runtime.spawn(async move {
        while let Some(entry) = execute_receiver.recv().await {
            if let Err(error) = executor.execute_app(&entry).await {
                eprintln!("Failed to launch {}: {error}", entry.name);
            }
        }
    });

    ui::run(query_sender, execute_sender, search_service.subscribe(), 8)
        .map_err(|error| core::AppError::execution_failed(format!("Iced UI failed: {error}")))?;
    Ok(())
}
