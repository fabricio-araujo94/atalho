//! Asynchronous search orchestration pipeline, index lifecycle, and debounced query execution.

use core::{
    AppEntry, AppError, History, Indexer, Matcher, Persistence, Result, ScoredMatch, SearchEvent,
    SearchRequest, SearchResponse,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Orchestration service coordinating indexing, in-memory caching, debouncing,
/// cancellation, and fast fuzzy matching.
pub struct SearchService {
    indexer: Arc<dyn Indexer>,
    matcher: Arc<dyn Matcher>,
    persistence: Arc<dyn Persistence>,
    history: Option<Arc<dyn History>>,
    cache: Arc<RwLock<Vec<AppEntry>>>,
    event_sender: broadcast::Sender<SearchEvent>,
    debounce_duration: Duration,
    active_search_token: Arc<std::sync::Mutex<Option<CancellationToken>>>,
    active_reindex_token: Arc<std::sync::Mutex<Option<CancellationToken>>>,
    reindex_commit_lock: Arc<tokio::sync::Mutex<()>>,
}

struct SearchTaskContext {
    cache: Arc<RwLock<Vec<AppEntry>>>,
    searcher: Arc<dyn Matcher>,
    history: Option<Arc<dyn History>>,
    event_tx: broadcast::Sender<SearchEvent>,
    active_search_token: Arc<std::sync::Mutex<Option<CancellationToken>>>,
}

impl SearchService {
    /// Default debounce window in milliseconds.
    pub const DEFAULT_DEBOUNCE_MS: u64 = 80;

    /// Default broadcast channel capacity for search events.
    pub const DEFAULT_CHANNEL_CAPACITY: usize = 32;

    /// Creates a new [`SearchService`] with default debounce window (80ms).
    #[must_use]
    pub fn new(
        indexer: Arc<dyn Indexer>,
        matcher: Arc<dyn Matcher>,
        persistence: Arc<dyn Persistence>,
    ) -> Self {
        Self::with_debounce(
            indexer,
            matcher,
            persistence,
            Duration::from_millis(Self::DEFAULT_DEBOUNCE_MS),
        )
    }

    /// Enables history-based usage ranking.
    #[must_use]
    pub fn with_history(mut self, history: Arc<dyn History>) -> Self {
        self.history = Some(history);
        self
    }

    /// Creates a new [`SearchService`] with a configurable debounce duration.
    #[must_use]
    pub fn with_debounce(
        indexer: Arc<dyn Indexer>,
        matcher: Arc<dyn Matcher>,
        persistence: Arc<dyn Persistence>,
        debounce_duration: Duration,
    ) -> Self {
        let (event_sender, _) = broadcast::channel(Self::DEFAULT_CHANNEL_CAPACITY);
        Self {
            indexer,
            matcher,
            persistence,
            history: None,
            cache: Arc::new(RwLock::new(Vec::new())),
            event_sender,
            debounce_duration,
            active_search_token: Arc::new(std::sync::Mutex::new(None)),
            active_reindex_token: Arc::new(std::sync::Mutex::new(None)),
            reindex_commit_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Subscribes to search lifecycle broadcast events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<SearchEvent> {
        self.event_sender.subscribe()
    }

    /// Returns a shared reference to the in-memory application cache.
    #[must_use]
    pub fn cache(&self) -> Arc<RwLock<Vec<AppEntry>>> {
        Arc::clone(&self.cache)
    }

    /// Initializes the application cache from persistence, triggering a background
    /// re-index if the cache is empty or stale.
    ///
    /// # Errors
    /// Returns an error if loading persistence data fails catastrophically.
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing SearchService application index cache...");

        let cached_apps = match self.persistence.load_apps().await {
            Ok(apps) => apps,
            Err(err) => {
                let _ = self.event_sender.send(SearchEvent::Error(err.clone()));
                return Err(err);
            }
        };
        let last_updated = match self.persistence.apps_last_updated().await {
            Ok(timestamp) => timestamp,
            Err(err) => {
                let _ = self.event_sender.send(SearchEvent::Error(err.clone()));
                return Err(err);
            }
        };
        let cache_is_stale = last_updated.is_none_or(|updated| {
            std::time::SystemTime::now()
                .duration_since(updated)
                .is_ok_and(|age| age > Duration::from_hours(24))
        });

        if !cached_apps.is_empty() {
            info!(
                "Loaded {} application entries from persistent cache",
                cached_apps.len()
            );
            let mut cache_guard = self.cache.write().await;
            *cache_guard = cached_apps;
        }

        if cache_is_stale || self.cache.read().await.is_empty() {
            info!("Application cache is missing or stale; triggering background indexing...");
            self.trigger_background_reindex();
        }

        Ok(())
    }

    /// Triggers an asynchronous background re-index off the caller's context.
    ///
    /// The UI continues serving currently cached results until background scanning
    /// completes and atomically replaces the in-memory cache and persistence storage.
    pub fn trigger_background_reindex(&self) {
        let indexer = Arc::clone(&self.indexer);
        let persistence = Arc::clone(&self.persistence);
        let cache = Arc::clone(&self.cache);
        let event_tx = self.event_sender.clone();
        let commit_lock = Arc::clone(&self.reindex_commit_lock);
        let token = CancellationToken::new();
        {
            let mut active = self
                .active_reindex_token
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(previous) = active.replace(token.clone()) {
                previous.cancel();
            }
        }

        tokio::spawn(async move {
            debug!("Starting background application re-index scan...");
            match indexer.scan_with_cancellation(token.clone()).await {
                Ok(Some(new_entries)) if !token.is_cancelled() => {
                    info!(
                        "Background scan completed successfully ({} apps discovered)",
                        new_entries.len()
                    );

                    let _commit_guard = commit_lock.lock().await;
                    if token.is_cancelled() {
                        return;
                    }
                    // Persist discovered entries
                    if let Err(err) = persistence.save_apps(&new_entries).await {
                        warn!("Failed to persist updated application entries: {err}");
                        let _ = event_tx.send(SearchEvent::Error(err));
                    }

                    if token.is_cancelled() {
                        return;
                    }
                    // Atomically replace in-memory cache
                    {
                        let mut cache_guard = cache.write().await;
                        *cache_guard = new_entries;
                    }
                    debug!("In-memory application cache atomically refreshed");
                }
                Ok(None) => debug!("Background re-index was cancelled"),
                Ok(Some(_)) => debug!("Background re-index was cancelled before cache refresh"),
                Err(err) => {
                    error!("Background indexer failed: {err}");
                    let _ = event_tx.send(SearchEvent::Error(err));
                }
            }
        });
    }

    /// Executes an immediate search without debouncing, evaluating the given [`SearchRequest`].
    ///
    /// # Errors
    /// Returns [`AppError`] if search execution fails.
    pub async fn search_immediate(&self, req: SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();

        // Snapshot current cache under read lock
        let entries = {
            let guard = self.cache.read().await;
            guard.clone()
        };

        let searcher = Arc::clone(&self.matcher);
        let query = req.query.clone();
        let max_results = req.max_results;

        // Perform fast synchronous matching inside spawn_blocking to prevent async runtime blocking
        let (mut all_matches, total) = tokio::task::spawn_blocking(move || {
            let scored_results = searcher.search(&query, &entries);
            let total = scored_results.len();
            (scored_results, total)
        })
        .await
        .map_err(|join_err| {
            AppError::execution_failed(format!("Search task failed to join: {join_err}"))
        })?;
        if let Some(history) = &self.history {
            let frequency = history.frequency().await?;
            Self::apply_history_boost(&mut all_matches, &frequency);
        }

        let elapsed = start.elapsed();
        let elapsed_us = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);

        let results: Vec<_> = all_matches.into_iter().take(max_results).collect();
        debug!(
            "Search for '{}' completed in {} us ({} total matches, returning {})",
            req.query,
            elapsed_us,
            total,
            results.len()
        );

        Ok(SearchResponse::new(results, total, elapsed_us))
    }

    /// Submits a query to the debounced search pipeline with cooperative cancellation.
    ///
    /// Every invocation cancels any active in-flight debounced search task via
    /// [`CancellationToken`] and resets the debounce timer (80ms).
    pub fn submit_query(&self, req: SearchRequest) {
        // Cancel prior in-flight search task
        let new_token = CancellationToken::new();
        {
            let mut lock = self
                .active_search_token
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(old_token) = lock.replace(new_token.clone()) {
                old_token.cancel();
            }
        }

        let event_tx = self.event_sender.clone();
        let debounce = self.debounce_duration;
        let cache = Arc::clone(&self.cache);
        let searcher = Arc::clone(&self.matcher);
        let history = self.history.clone();
        let active_search_token = Arc::clone(&self.active_search_token);
        let context = SearchTaskContext {
            cache,
            searcher,
            history,
            event_tx,
            active_search_token,
        };

        tokio::spawn(async move {
            Self::run_debounced_search(req, new_token, debounce, context).await;
        });
    }

    async fn run_debounced_search(
        req: SearchRequest,
        token: CancellationToken,
        debounce: Duration,
        context: SearchTaskContext,
    ) {
        let SearchTaskContext {
            cache,
            searcher,
            history,
            event_tx,
            active_search_token,
        } = context;
        {
            let _active = active_search_token
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !token.is_cancelled() {
                let _ = event_tx.send(SearchEvent::Loading);
            }
        }

        tokio::select! {
            () = tokio::time::sleep(debounce) => {}
            () = token.cancelled() => {
                debug!("Debounced search cancelled before timer expired for query '{}'", req.query);
                return;
            }
        }
        if token.is_cancelled() {
            return;
        }

        let start = Instant::now();
        let entries = cache.read().await.clone();
        let query = req.query.clone();
        let max_results = req.max_results;
        let token_for_match = token.clone();
        let match_res = tokio::task::spawn_blocking(move || {
            searcher
                .search_with_cancellation(&query, &entries, &token_for_match)
                .map(|matches| {
                    let total = matches.len();
                    (matches, total)
                })
        })
        .await;

        if token.is_cancelled() {
            debug!(
                "Search cancelled during or immediately after matching for query '{}'",
                req.query
            );
            return;
        }

        match match_res {
            Ok(Some((mut scored_results, total))) => {
                if let Some(history) = history {
                    match history.frequency().await {
                        Ok(frequency) => {
                            Self::apply_history_boost(&mut scored_results, &frequency);
                        }
                        Err(error) => {
                            let _active = active_search_token
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            if !token.is_cancelled() {
                                let _ = event_tx.send(SearchEvent::Error(error));
                            }
                            return;
                        }
                    }
                }
                let elapsed_us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);
                let results: Vec<_> = scored_results.into_iter().take(max_results).collect();
                debug!(
                    "Search for '{}' completed in {} us ({} total matches, returning {})",
                    req.query,
                    elapsed_us,
                    total,
                    results.len()
                );
                let response = SearchResponse::new(results, total, elapsed_us);
                let _active = active_search_token
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if !token.is_cancelled() {
                    let _ = event_tx.send(SearchEvent::Results(response));
                }
            }
            Ok(None) => {
                debug!("Search cancelled internally for query '{}'", req.query);
            }
            Err(join_err) => {
                let err =
                    AppError::execution_failed(format!("Search task failed to join: {join_err}"));
                let _active = active_search_token
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if !token.is_cancelled() {
                    let _ = event_tx.send(SearchEvent::Error(err));
                }
            }
        }
    }

    fn apply_history_boost(matches: &mut [ScoredMatch], frequency: &HashMap<uuid::Uuid, u32>) {
        for candidate in matches.iter_mut() {
            if let Some(count) = frequency.get(&candidate.entry.id) {
                let boost = f64::from(*count).ln_1p() * 8.0;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let boost = boost.round() as i64;
                candidate.score = candidate.score.saturating_add(boost);
            }
        }
        matches.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
    }

    /// Exposes `async fn search(req: SearchRequest) -> Result<SearchResponse>` as specified
    /// in Architectural Directive 2.
    ///
    /// # Errors
    /// Returns [`AppError`] if search execution fails.
    pub async fn search(&self, req: SearchRequest) -> Result<SearchResponse> {
        self.search_immediate(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{AppSource, Indexer, Matcher, ScoredMatch};
    use persistence::InMemoryPersistence;

    struct DummyIndexer {
        apps: Vec<AppEntry>,
    }

    #[async_trait::async_trait]
    impl Indexer for DummyIndexer {
        async fn scan(&self) -> Result<Vec<AppEntry>> {
            Ok(self.apps.clone())
        }
    }

    struct SlowIndexer {
        apps: Vec<AppEntry>,
    }

    #[async_trait::async_trait]
    impl Indexer for SlowIndexer {
        async fn scan(&self) -> Result<Vec<AppEntry>> {
            tokio::time::sleep(Duration::from_millis(40)).await;
            Ok(self.apps.clone())
        }
    }

    struct StalePersistence {
        apps: Vec<AppEntry>,
    }

    #[async_trait::async_trait]
    impl Persistence for StalePersistence {
        async fn save_apps(&self, _apps: &[AppEntry]) -> Result<()> {
            Ok(())
        }

        async fn load_apps(&self) -> Result<Vec<AppEntry>> {
            Ok(self.apps.clone())
        }

        async fn apps_last_updated(&self) -> Result<Option<std::time::SystemTime>> {
            Ok(Some(std::time::SystemTime::UNIX_EPOCH))
        }

        async fn record_launch(&self, _entry_id: &str) -> Result<()> {
            Ok(())
        }
    }

    struct SimpleMatcher;

    impl Matcher for SimpleMatcher {
        fn search(&self, query: &str, entries: &[AppEntry]) -> Vec<ScoredMatch> {
            entries
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&query.to_lowercase()))
                .map(|e| ScoredMatch::new(e.clone(), 100, Vec::new()))
                .collect()
        }
    }

    #[tokio::test]
    async fn test_search_service_immediate_search() {
        let app1 = AppEntry::new("Calculator", "calc.exe", AppSource::StartMenu);
        let app2 = AppEntry::new("Notepad", "notepad.exe", AppSource::StartMenu);

        let indexer = Arc::new(DummyIndexer {
            apps: vec![app1.clone(), app2.clone()],
        });
        let matcher = Arc::new(SimpleMatcher);
        let persistence = Arc::new(InMemoryPersistence::new());

        let service = SearchService::new(indexer, matcher, persistence);
        if let Err(err) = service.initialize().await {
            panic!("service initialization failed: {err}");
        }

        // Wait for background scan to populate cache
        tokio::time::sleep(Duration::from_millis(50)).await;

        let response = match service.search(SearchRequest::new("calc", 10)).await {
            Ok(response) => response,
            Err(err) => panic!("search failed: {err}"),
        };

        assert_eq!(response.total, 1);
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].entry.name, "Calculator");
    }

    #[tokio::test]
    async fn test_search_service_debounce_and_cancellation() {
        let app1 = AppEntry::new("Chrome", "chrome.exe", AppSource::StartMenu);
        let app2 = AppEntry::new("Chromium", "chromium.exe", AppSource::StartMenu);

        let indexer = Arc::new(DummyIndexer {
            apps: vec![app1, app2],
        });
        let matcher = Arc::new(SimpleMatcher);
        let persistence = Arc::new(InMemoryPersistence::new());

        let service =
            SearchService::with_debounce(indexer, matcher, persistence, Duration::from_millis(50));
        if let Err(err) = service.initialize().await {
            panic!("service initialization failed: {err}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut rx = service.subscribe();

        // Submit query 1, then immediately cancel with query 2
        service.submit_query(SearchRequest::new("Chr", 10));
        tokio::time::sleep(Duration::from_millis(10)).await;
        service.submit_query(SearchRequest::new("Chromium", 10));

        // Consume events
        let mut got_chromium_result = false;
        let timeout = tokio::time::sleep(Duration::from_millis(200));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                ev = rx.recv() => {
                    if let Ok(SearchEvent::Results(res)) = ev
                        && res.results.len() == 1 && res.results[0].entry.name == "Chromium"
                    {
                        got_chromium_result = true;
                        break;
                    }
                }
                () = &mut timeout => {
                    break;
                }
            }
        }

        assert!(got_chromium_result);
    }

    #[tokio::test]
    async fn stale_cache_remains_available_until_reindex_finishes() {
        let stale_entry = AppEntry::new("Old App", "old.exe", AppSource::StartMenu);
        let fresh_entry = AppEntry::new("New App", "new.exe", AppSource::StartMenu);
        let service = SearchService::new(
            Arc::new(SlowIndexer {
                apps: vec![fresh_entry.clone()],
            }),
            Arc::new(SimpleMatcher),
            Arc::new(StalePersistence {
                apps: vec![stale_entry.clone()],
            }),
        );

        if let Err(err) = service.initialize().await {
            panic!("service initialization failed: {err}");
        }
        assert_eq!(service.cache.read().await.as_slice(), &[stale_entry]);

        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        loop {
            if service.cache.read().await.as_slice() == [fresh_entry.clone()] {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "re-index timed out");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[test]
    fn launch_frequency_boost_reorders_matches() {
        let less_used = AppEntry::new("Less Used", "less-used.exe", AppSource::StartMenu);
        let frequently_used = AppEntry::new(
            "Frequently Used",
            "frequently-used.exe",
            AppSource::StartMenu,
        );
        let mut matches = vec![
            ScoredMatch::new(less_used, 100, Vec::new()),
            ScoredMatch::new(frequently_used.clone(), 100, Vec::new()),
        ];
        let frequency = HashMap::from([(frequently_used.id, 100)]);

        SearchService::apply_history_boost(&mut matches, &frequency);

        assert_eq!(matches[0].entry.name, "Frequently Used");
    }
}
