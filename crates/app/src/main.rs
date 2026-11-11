//! # Atalho Application Entry Point
//!
//! Composition root that owns the launcher runtime, tray, persistence, and
//! orderly shutdown sequence.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use app::search::SearchService;
use core::{AppConfig, Config, Executor, GlobalHotkey, History, Matcher, Persistence};
use executor::SystemExecutor;
use indexer::DefaultIndexer;
use matcher::FuzzyMatcher;
#[cfg(windows)]
use persistence::FilePersistence;
#[cfg(not(windows))]
use persistence::InMemoryPersistence;
use platform::{PlatformManager, SingleInstanceGuard, TrayController, TrayEvent};
use std::sync::{Arc, mpsc as std_mpsc};
use std::thread::JoinHandle;
use tokio::sync::mpsc;
use tracing::{error, warn};
use ui::UiCommand;

fn main() -> core::Result<()> {
    let Some(mut instance) = SingleInstanceGuard::acquire()? else {
        return Ok(());
    };

    let (tray_event_sender, tray_event_receiver) = std_mpsc::channel();
    let tray = TrayController::start(tray_event_sender.clone())?;
    instance.listen_for_show(tray_event_sender)?;

    let (ui_command_sender, ui_command_receiver) = mpsc::unbounded_channel();
    let tray_event_bridge =
        start_tray_event_bridge(tray_event_receiver, ui_command_sender.clone())?;

    let platform = PlatformManager::new();
    println!("Platform detected: {:?}", platform.platform_kind());

    #[cfg(windows)]
    let persistence = Arc::new(FilePersistence::new()?);
    #[cfg(not(windows))]
    let persistence = Arc::new(InMemoryPersistence::new());
    let app_persistence: Arc<dyn Persistence> = persistence.clone();
    let history: Arc<dyn History> = persistence.clone();
    let config_store: Arc<dyn Config> = persistence;
    let config = config_store.load()?;

    let indexer = default_indexer();
    let matcher = Arc::new(FuzzyMatcher::new());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            core::AppError::execution_failed(format!("Tokio startup failed: {error}"))
        })?;
    let search_service = Arc::new(
        SearchService::new(
            indexer,
            Arc::clone(&matcher) as Arc<dyn Matcher>,
            Arc::clone(&app_persistence),
        )
        .with_history(Arc::clone(&history)),
    );
    let executor = Arc::new(
        SystemExecutor::with_platform(platform.clone()).with_history(Arc::clone(&history)),
    );
    runtime.block_on(search_service.initialize())?;

    let (query_sender, mut query_receiver) = mpsc::unbounded_channel();
    let query_worker = {
        let service = Arc::clone(&search_service);
        runtime.spawn(async move {
            while let Some(request) = query_receiver.recv().await {
                service.submit_query(request);
            }
        })
    };

    let (execute_sender, mut execute_receiver) = mpsc::unbounded_channel();
    let launch_service = Arc::clone(&search_service);
    let launch_worker = runtime.spawn(async move {
        while let Some(entry) = execute_receiver.recv().await {
            if let Err(error) = executor.execute_app(&entry).await {
                launch_service.report_error(error);
            }
        }
    });

    let (auto_start_sender, auto_start_worker) = start_auto_start_worker(
        &runtime,
        config.clone(),
        Arc::clone(&config_store),
        Arc::clone(&search_service),
        ui_command_sender.clone(),
    );

    let ui_result = ui::run(
        query_sender,
        execute_sender,
        search_service.subscribe(),
        ui_command_receiver,
        auto_start_sender.clone(),
        config.max_results,
        config.auto_start,
    )
    .map_err(|error| core::AppError::execution_failed(format!("Iced UI failed: {error}")));

    shutdown(ShutdownContext {
        runtime,
        search_service,
        auto_start_sender,
        ui_command_sender,
        query_worker,
        launch_worker,
        auto_start_worker,
        app_persistence,
        history,
        config_store,
        platform,
        instance,
        tray,
        tray_event_bridge,
    });
    ui_result
}

struct ShutdownContext {
    runtime: tokio::runtime::Runtime,
    search_service: Arc<SearchService>,
    auto_start_sender: mpsc::UnboundedSender<bool>,
    ui_command_sender: mpsc::UnboundedSender<UiCommand>,
    query_worker: tokio::task::JoinHandle<()>,
    launch_worker: tokio::task::JoinHandle<()>,
    auto_start_worker: tokio::task::JoinHandle<()>,
    app_persistence: Arc<dyn Persistence>,
    history: Arc<dyn History>,
    config_store: Arc<dyn Config>,
    platform: PlatformManager,
    instance: SingleInstanceGuard,
    tray: TrayController,
    tray_event_bridge: JoinHandle<()>,
}

fn shutdown(context: ShutdownContext) {
    // Best effort: each cleanup step runs even if a previous one fails.
    let ShutdownContext {
        runtime,
        search_service,
        auto_start_sender,
        ui_command_sender,
        query_worker,
        launch_worker,
        auto_start_worker,
        app_persistence,
        history,
        config_store,
        mut platform,
        mut instance,
        mut tray,
        tray_event_bridge,
    } = context;
    search_service.cancel_tasks();
    drop(auto_start_sender);
    drop(ui_command_sender);
    runtime.block_on(async {
        for (name, result) in [
            (
                "query worker",
                query_worker.await.map_err(|error| error.to_string()),
            ),
            (
                "launch worker",
                launch_worker.await.map_err(|error| error.to_string()),
            ),
            (
                "auto-start worker",
                auto_start_worker.await.map_err(|error| error.to_string()),
            ),
        ] {
            if let Err(error) = result {
                error!("{name} did not stop cleanly: {error}");
            }
        }
        let cached_apps = search_service.cache().read().await.clone();
        if let Err(error) = app_persistence.save_apps(&cached_apps).await {
            error!("Could not flush the application index: {error}");
        }
        if let Err(error) = history.flush().await {
            error!("Could not flush launch history: {error}");
        }
    });
    match config_store.load() {
        Ok(final_config) => {
            if let Err(error) = config_store.save(&final_config) {
                error!("Could not save launcher configuration: {error}");
            }
        }
        Err(error) => error!("Could not load launcher configuration for shutdown save: {error}"),
    }
    if let Err(error) = platform.unregister_all() {
        error!("Could not unregister launcher hotkeys: {error}");
    }
    if let Err(error) = instance.stop_listening() {
        error!("Could not stop the single-instance listener: {error}");
    }
    if let Err(error) = tray.shutdown() {
        error!("Could not remove the launcher tray icon: {error}");
    }
    if tray_event_bridge.join().is_err() {
        warn!("Tray event bridge did not stop cleanly");
    }
    drop(instance);
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
}

fn start_tray_event_bridge(
    receiver: std_mpsc::Receiver<TrayEvent>,
    ui_sender: mpsc::UnboundedSender<UiCommand>,
) -> core::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("launcher-tray-event-bridge".to_owned())
        .spawn(move || {
            while let Ok(event) = receiver.recv() {
                let command = match event {
                    TrayEvent::ToggleVisibility => UiCommand::ToggleVisibility,
                    TrayEvent::Show => UiCommand::Show,
                    TrayEvent::Settings => UiCommand::OpenSettings,
                    TrayEvent::Quit => UiCommand::Quit,
                };
                if ui_sender.send(command).is_err() {
                    break;
                }
            }
        })
        .map_err(|error| {
            core::AppError::execution_failed(format!("Tray event bridge startup failed: {error}"))
        })
}

fn default_indexer() -> Arc<DefaultIndexer> {
    Arc::new(DefaultIndexer::new())
}

fn start_auto_start_worker(
    runtime: &tokio::runtime::Runtime,
    config: AppConfig,
    config_store: Arc<dyn Config>,
    service: Arc<SearchService>,
    ui_sender: mpsc::UnboundedSender<UiCommand>,
) -> (mpsc::UnboundedSender<bool>, tokio::task::JoinHandle<()>) {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let worker = runtime.spawn(async move {
        let mut current_config = config;
        while let Some(enabled) = receiver.recv().await {
            let previous = current_config.auto_start;
            let os_result = tokio::task::spawn_blocking(move || platform::set_auto_start(enabled))
                .await
                .map_err(|error| {
                    core::AppError::execution_failed(format!(
                        "Auto-start registration task failed: {error}"
                    ))
                })
                .and_then(std::convert::identity);
            if let Err(error) = os_result {
                service.report_error(error);
                let _ = ui_sender.send(UiCommand::AutoStartState(previous));
                continue;
            }

            let mut updated = current_config.clone();
            updated.auto_start = enabled;
            let config_for_save = Arc::clone(&config_store);
            let save_result = tokio::task::spawn_blocking(move || config_for_save.save(&updated))
                .await
                .map_err(|error| {
                    core::AppError::execution_failed(format!(
                        "Auto-start configuration save task failed: {error}"
                    ))
                })
                .and_then(std::convert::identity);
            if let Err(error) = save_result {
                match tokio::task::spawn_blocking(move || platform::set_auto_start(previous)).await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(rollback_error)) => {
                        error!("Could not restore prior auto-start registration: {rollback_error}");
                    }
                    Err(join_error) => {
                        error!("Auto-start rollback task failed to join: {join_error}");
                    }
                }
                service.report_error(error);
                let _ = ui_sender.send(UiCommand::AutoStartState(previous));
            } else {
                current_config.auto_start = enabled;
                let _ = ui_sender.send(UiCommand::AutoStartState(enabled));
            }
        }
    });
    (sender, worker)
}
