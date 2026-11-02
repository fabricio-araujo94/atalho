//! On-disk persistence and lightweight in-memory test doubles.

use core::{
    AppConfig, AppEntry, AppError, CacheMetadata, Config, History, IndexCache, Persistence, Result,
};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
#[cfg(any(test, feature = "test-utils"))]
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

const INDEX_FILE: &str = "index.bin.zst";
const HISTORY_FILE: &str = "history.bin";
const ROTATED_HISTORY_FILE: &str = "history.1.bin";
const CONFIG_FILE: &str = "config.toml";
const HISTORY_ROTATE_ENTRIES: u64 = 10_000;
const HISTORY_RECORD_SIZE: usize = 24;

#[derive(Debug, Clone, Copy)]
struct HistoryRecord {
    id: Uuid,
}

/// Persistent storage rooted in `%LOCALAPPDATA%\\FlowLauncherRust`.
///
/// Use [`FilePersistence::from_dir`] to select a custom directory, for example in tests.
pub struct FilePersistence {
    root: PathBuf,
    cache_lock: AsyncMutex<()>,
    history_lock: AsyncMutex<()>,
    config_lock: Mutex<()>,
}

impl std::fmt::Debug for FilePersistence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FilePersistence")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl FilePersistence {
    /// Resolves the platform-specific application data directory.
    ///
    /// # Errors
    /// Returns an I/O error with path context if `LOCALAPPDATA` is unavailable.
    pub fn new() -> Result<Self> {
        let local_app_data = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
            AppError::io_failed("LOCALAPPDATA is not set; cannot resolve persistence directory")
        })?;
        Ok(Self::from_dir(
            PathBuf::from(local_app_data).join("FlowLauncherRust"),
        ))
    }

    /// Creates persistence rooted at the supplied directory.
    #[must_use]
    pub fn from_dir(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            cache_lock: AsyncMutex::new(()),
            history_lock: AsyncMutex::new(()),
            config_lock: Mutex::new(()),
        }
    }

    fn file_path(&self, file_name: &str) -> PathBuf {
        self.root.join(file_name)
    }

    async fn save_index(&self, entries: &[AppEntry]) -> Result<()> {
        let path = self.file_path(INDEX_FILE);
        let path_context = path.display().to_string();
        let entries = entries.to_vec();
        let bytes = tokio::task::spawn_blocking(move || {
            let serialized = bincode::serialize(&entries).map_err(|error| {
                AppError::io_failed(format!("{path_context}: serialize index: {error}"))
            })?;
            zstd::stream::encode_all(serialized.as_slice(), 0).map_err(|error| {
                AppError::io_failed(format!("{path_context}: compress index: {error}"))
            })
        })
        .await
        .map_err(|error| {
            AppError::io_failed(format!("index compression task failed: {error}"))
        })??;
        atomic_write(&path, &bytes).await
    }

    async fn load_index(&self) -> Result<Option<Vec<AppEntry>>> {
        let path = self.file_path(INDEX_FILE);
        let compressed = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(path_error(&path, error)),
        };
        tokio::task::spawn_blocking(move || {
            let decompressed =
                zstd::stream::decode_all(compressed.as_slice()).map_err(|error| {
                    AppError::io_failed(format!("{}: decompress: {error}", path.display()))
                })?;
            bincode::deserialize(&decompressed)
                .map(Some)
                .map_err(|error| {
                    AppError::io_failed(format!("{}: deserialize: {error}", path.display()))
                })
        })
        .await
        .map_err(|error| AppError::io_failed(format!("index decompression task failed: {error}")))?
    }

    async fn history_records(&self) -> Result<Vec<HistoryRecord>> {
        let mut records = Vec::new();
        for file_name in [ROTATED_HISTORY_FILE, HISTORY_FILE] {
            let path = self.file_path(file_name);
            let bytes = match tokio::fs::read(&path).await {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(path_error(&path, error)),
            };
            if bytes.len() % HISTORY_RECORD_SIZE != 0 {
                return Err(AppError::io_failed(format!(
                    "{}: incomplete history record",
                    path.display()
                )));
            }
            let (chunks, _) = bytes.as_chunks::<HISTORY_RECORD_SIZE>();
            records.extend(chunks.iter().map(|chunk| {
                let id = Uuid::from_bytes(chunk[8..].try_into().unwrap_or([0; 16]));
                HistoryRecord { id }
            }));
        }
        Ok(records)
    }

    async fn record_history(&self, entry_id: Uuid) -> Result<()> {
        let path = self.file_path(HISTORY_FILE);
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|error| path_error(&self.root, error))?;
        let count = match tokio::fs::metadata(&path).await {
            Ok(metadata) => {
                let len = metadata.len();
                if len % HISTORY_RECORD_SIZE as u64 != 0 {
                    return Err(AppError::io_failed(format!(
                        "{}: incomplete history record",
                        path.display()
                    )));
                }
                len / HISTORY_RECORD_SIZE as u64
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(path_error(&path, error)),
        };
        if count >= HISTORY_ROTATE_ENTRIES {
            let rotated = self.file_path(ROTATED_HISTORY_FILE);
            match tokio::fs::remove_file(&rotated).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(path_error(&rotated, error)),
            }
            tokio::fs::rename(&path, &rotated)
                .await
                .map_err(|error| path_error(&rotated, error))?;
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| AppError::io_failed(format!("system clock: {error}")))?
            .as_secs();
        let mut record = [0_u8; HISTORY_RECORD_SIZE];
        record[..8].copy_from_slice(&timestamp.to_le_bytes());
        record[8..].copy_from_slice(entry_id.as_bytes());
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .map_err(|error| path_error(&path, error))?;
        file.write_all(&record)
            .await
            .map_err(|error| path_error(&path, error))?;
        file.flush().await.map_err(|error| path_error(&path, error))
    }

    fn run_config_io<T, F, Fut>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(PathBuf) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T>> + Send + 'static,
    {
        let root = self.root.clone();
        let worker = std::thread::Builder::new()
            .name("persistence-config-io".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| {
                        AppError::io_failed(format!(
                            "{}: create config I/O runtime: {error}",
                            root.display()
                        ))
                    })?;
                runtime.block_on(operation(root))
            })
            .map_err(|error| AppError::io_failed(format!("start config I/O thread: {error}")))?;
        worker
            .join()
            .map_err(|_| AppError::io_failed("config I/O thread panicked"))?
    }
}

#[async_trait::async_trait]
impl IndexCache for FilePersistence {
    async fn load(&self) -> Result<Option<Vec<AppEntry>>> {
        let _guard = self.cache_lock.lock().await;
        self.load_index().await
    }

    async fn save(&self, entries: &[AppEntry]) -> Result<()> {
        let _guard = self.cache_lock.lock().await;
        self.save_index(entries).await
    }

    async fn metadata(&self) -> Result<CacheMetadata> {
        let _guard = self.cache_lock.lock().await;
        let path = self.file_path(INDEX_FILE);
        let timestamp = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata
                .modified()
                .map_err(|error| path_error(&path, error))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => UNIX_EPOCH,
            Err(error) => return Err(path_error(&path, error)),
        };
        let count = self.load_index().await?.map_or(0, |entries| entries.len());
        Ok(CacheMetadata { timestamp, count })
    }
}

#[async_trait::async_trait]
impl History for FilePersistence {
    async fn record(&self, entry_id: Uuid) -> Result<()> {
        let _guard = self.history_lock.lock().await;
        self.record_history(entry_id).await
    }

    async fn recent(&self, limit: usize) -> Result<Vec<Uuid>> {
        let _guard = self.history_lock.lock().await;
        let records = self.history_records().await?;
        Ok(records
            .into_iter()
            .rev()
            .take(limit)
            .map(|record| record.id)
            .collect())
    }

    async fn frequency(&self) -> Result<HashMap<Uuid, u32>> {
        let _guard = self.history_lock.lock().await;
        let mut counts = HashMap::new();
        for record in self.history_records().await? {
            let count = counts.entry(record.id).or_insert(0_u32);
            *count = count.saturating_add(1);
        }
        Ok(counts)
    }
}

impl Config for FilePersistence {
    fn load(&self) -> Result<AppConfig> {
        let _guard = self
            .config_lock
            .lock()
            .map_err(|error| AppError::io_failed(format!("config lock: {error}")))?;
        self.run_config_io(|root| async move {
            let path = root.join(CONFIG_FILE);
            let bytes = match tokio::fs::read(&path).await {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(AppConfig::default());
                }
                Err(error) => return Err(path_error(&path, error)),
            };
            let contents = String::from_utf8(bytes).map_err(|error| {
                AppError::io_failed(format!("{}: config is not UTF-8: {error}", path.display()))
            })?;
            toml::from_str(&contents).map_err(|error| {
                AppError::io_failed(format!("{}: parse TOML: {error}", path.display()))
            })
        })
    }

    fn save(&self, cfg: &AppConfig) -> Result<()> {
        let _guard = self
            .config_lock
            .lock()
            .map_err(|error| AppError::io_failed(format!("config lock: {error}")))?;
        let serialized = toml::to_string_pretty(cfg).map_err(|error| {
            AppError::io_failed(format!(
                "{}: serialize config TOML: {error}",
                self.file_path(CONFIG_FILE).display()
            ))
        })?;
        self.run_config_io(move |root| async move {
            atomic_write(&root.join(CONFIG_FILE), serialized.as_bytes()).await
        })
    }
}

#[async_trait::async_trait]
impl Persistence for FilePersistence {
    async fn save_apps(&self, apps: &[AppEntry]) -> Result<()> {
        IndexCache::save(self, apps).await
    }

    async fn load_apps(&self) -> Result<Vec<AppEntry>> {
        Ok(IndexCache::load(self).await?.unwrap_or_default())
    }

    async fn apps_last_updated(&self) -> Result<Option<SystemTime>> {
        let metadata = IndexCache::metadata(self).await?;
        Ok((metadata.count > 0).then_some(metadata.timestamp))
    }

    async fn record_launch(&self, entry_id: &str) -> Result<()> {
        let id = Uuid::parse_str(entry_id).map_err(|error| {
            AppError::io_failed(format!(
                "{}: invalid history entry ID: {error}",
                self.file_path(HISTORY_FILE).display()
            ))
        })?;
        History::record(self, id).await
    }
}

/// In-memory implementation of [`IndexCache`] for tests and ephemeral use.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Default)]
pub struct InMemoryCache {
    entries: RwLock<Option<Vec<AppEntry>>>,
    timestamp: RwLock<Option<SystemTime>>,
}

#[cfg(any(test, feature = "test-utils"))]
#[async_trait::async_trait]
impl IndexCache for InMemoryCache {
    async fn load(&self) -> Result<Option<Vec<AppEntry>>> {
        self.entries
            .read()
            .map(|guard| guard.clone())
            .map_err(|error| AppError::io_failed(format!("memory cache lock: {error}")))
    }

    async fn save(&self, entries: &[AppEntry]) -> Result<()> {
        *self
            .entries
            .write()
            .map_err(|error| AppError::io_failed(format!("memory cache lock: {error}")))? =
            Some(entries.to_vec());
        *self
            .timestamp
            .write()
            .map_err(|error| AppError::io_failed(format!("memory cache lock: {error}")))? =
            Some(SystemTime::now());
        Ok(())
    }

    async fn metadata(&self) -> Result<CacheMetadata> {
        let entries = self.load().await?;
        let timestamp = self
            .timestamp
            .read()
            .map_err(|error| AppError::io_failed(format!("memory cache lock: {error}")))?
            .unwrap_or(UNIX_EPOCH);
        Ok(CacheMetadata {
            timestamp,
            count: entries.map_or(0, |entries| entries.len()),
        })
    }
}

/// In-memory implementation of [`History`] for tests and ephemeral use.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Default)]
pub struct InMemoryHistory {
    records: RwLock<Vec<(SystemTime, Uuid)>>,
}

#[cfg(any(test, feature = "test-utils"))]
#[async_trait::async_trait]
impl History for InMemoryHistory {
    async fn record(&self, entry_id: Uuid) -> Result<()> {
        self.records
            .write()
            .map_err(|error| AppError::io_failed(format!("memory history lock: {error}")))?
            .push((SystemTime::now(), entry_id));
        Ok(())
    }

    async fn recent(&self, limit: usize) -> Result<Vec<Uuid>> {
        let records = self
            .records
            .read()
            .map_err(|error| AppError::io_failed(format!("memory history lock: {error}")))?;
        Ok(records
            .iter()
            .rev()
            .take(limit)
            .map(|(_, id)| *id)
            .collect())
    }

    async fn frequency(&self) -> Result<HashMap<Uuid, u32>> {
        let records = self
            .records
            .read()
            .map_err(|error| AppError::io_failed(format!("memory history lock: {error}")))?;
        let mut counts = HashMap::new();
        for (_, id) in records.iter() {
            let count = counts.entry(*id).or_insert(0_u32);
            *count = count.saturating_add(1);
        }
        drop(records);
        Ok(counts)
    }
}

async fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        AppError::io_failed(format!(
            "{}: target has no parent directory",
            path.display()
        ))
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| path_error(parent, error))?;
    let mut temporary_name = path
        .file_name()
        .ok_or_else(|| AppError::io_failed(format!("{}: target has no file name", path.display())))?
        .to_os_string();
    temporary_name.push(format!(".{}.tmp", Uuid::new_v4()));
    let temporary_path = parent.join(temporary_name);
    let mut temporary_file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .await
        .map_err(|error| path_error(&temporary_path, error))?;
    if let Err(error) = temporary_file.write_all(contents).await {
        drop(temporary_file);
        return Err(cleanup_temp_after_write(&temporary_path, error).await);
    }
    if let Err(error) = temporary_file.sync_all().await {
        drop(temporary_file);
        return Err(cleanup_temp_after_write(&temporary_path, error).await);
    }
    drop(temporary_file);
    if let Err(error) = tokio::fs::rename(&temporary_path, path).await {
        return Err(cleanup_temp_after_rename(path, &temporary_path, error).await);
    }
    Ok(())
}

async fn cleanup_temp_after_write(target: &Path, error: std::io::Error) -> AppError {
    let message = error.to_string();
    match tokio::fs::remove_file(target).await {
        Ok(()) => path_error(target, error),
        Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
            path_error(target, error)
        }
        Err(cleanup_error) => AppError::io_failed(format!(
            "{}: {message}; cleanup of temporary file also failed: {cleanup_error}",
            target.display()
        )),
    }
}

async fn cleanup_temp_after_rename(
    target: &Path,
    temporary_path: &Path,
    error: std::io::Error,
) -> AppError {
    let message = error.to_string();
    match tokio::fs::remove_file(temporary_path).await {
        Ok(()) => path_error(target, error),
        Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
            path_error(target, error)
        }
        Err(cleanup_error) => AppError::io_failed(format!(
            "{}: {message}; cleanup of {} also failed: {cleanup_error}",
            target.display(),
            temporary_path.display()
        )),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn path_error(path: &Path, error: std::io::Error) -> AppError {
    let kind = error.kind();
    let message = error.to_string();
    AppError::IoFailed(std::io::Error::new(
        kind,
        format!("{}: {message}", path.display()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{AppSource, ThemeName};

    fn test_directory() -> PathBuf {
        std::env::temp_dir().join(format!("atalho-persistence-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn index_cache_round_trips_and_reports_metadata() {
        let directory = test_directory();
        let persistence = FilePersistence::from_dir(&directory);
        let apps = vec![AppEntry::new(
            "Editor",
            "C:\\Apps\\editor.exe",
            AppSource::Path,
        )];

        assert_eq!(
            IndexCache::load(&persistence).await.unwrap_or_default(),
            None
        );
        assert!(IndexCache::save(&persistence, &apps).await.is_ok());
        assert_eq!(
            IndexCache::load(&persistence).await.unwrap_or_default(),
            Some(apps.clone())
        );
        let metadata = IndexCache::metadata(&persistence).await;
        assert_eq!(metadata.map(|value| value.count).ok(), Some(1));
        assert!(
            tokio::fs::try_exists(directory.join(INDEX_FILE))
                .await
                .unwrap_or(false)
        );
        let _ = tokio::fs::remove_dir_all(directory).await;
    }

    #[tokio::test]
    async fn history_records_recent_order_and_frequency() {
        let directory = test_directory();
        let persistence = FilePersistence::from_dir(&directory);
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();

        assert!(History::record(&persistence, first).await.is_ok());
        assert!(History::record(&persistence, second).await.is_ok());
        assert!(History::record(&persistence, first).await.is_ok());
        assert_eq!(
            History::recent(&persistence, 2).await.ok(),
            Some(vec![first, second])
        );
        assert_eq!(
            History::frequency(&persistence)
                .await
                .ok()
                .and_then(|counts| counts.get(&first).copied()),
            Some(2)
        );
        let _ = tokio::fs::remove_dir_all(directory).await;
    }

    #[tokio::test]
    async fn history_rotates_after_ten_thousand_records() {
        let directory = test_directory();
        let persistence = FilePersistence::from_dir(&directory);
        let current = Uuid::new_v4();
        let history_path = directory.join(HISTORY_FILE);
        let contents = vec![0_u8; HISTORY_RECORD_SIZE * 10_000];
        assert!(tokio::fs::create_dir_all(&directory).await.is_ok());
        assert!(tokio::fs::write(&history_path, contents).await.is_ok());

        assert!(History::record(&persistence, current).await.is_ok());
        let active_len = tokio::fs::metadata(history_path)
            .await
            .map(|metadata| metadata.len())
            .ok();
        let rotated_len = tokio::fs::metadata(directory.join(ROTATED_HISTORY_FILE))
            .await
            .map(|metadata| metadata.len())
            .ok();
        let record_size = u64::try_from(HISTORY_RECORD_SIZE).unwrap_or(u64::MAX);
        assert_eq!(active_len, Some(record_size));
        assert_eq!(rotated_len, Some(record_size * HISTORY_ROTATE_ENTRIES));
        assert_eq!(
            History::recent(&persistence, 1).await.ok(),
            Some(vec![current])
        );
        let _ = tokio::fs::remove_dir_all(directory).await;
    }

    #[tokio::test]
    async fn config_defaults_and_toml_round_trip() {
        let directory = test_directory();
        let persistence = FilePersistence::from_dir(&directory);
        assert_eq!(Config::load(&persistence).ok(), Some(AppConfig::default()));

        let config = AppConfig {
            theme: ThemeName::Solarized,
            max_results: 12,
            ..AppConfig::default()
        };
        assert!(Config::save(&persistence, &config).is_ok());
        assert_eq!(Config::load(&persistence).ok(), Some(config));
        let contents = tokio::fs::read_to_string(directory.join(CONFIG_FILE)).await;
        assert!(contents.is_ok_and(|contents| contents.contains("Solarized")));
        let _ = tokio::fs::remove_dir_all(directory).await;
    }

    #[tokio::test]
    async fn in_memory_implementations_cover_core_contracts() {
        let cache = InMemoryCache::default();
        let history = InMemoryHistory::default();
        let entry = AppEntry::new("Editor", "editor.exe", AppSource::Path);
        let id = entry.id;

        assert!(
            IndexCache::save(&cache, std::slice::from_ref(&entry))
                .await
                .is_ok()
        );
        assert_eq!(IndexCache::load(&cache).await.ok(), Some(Some(vec![entry])));
        assert!(History::record(&history, id).await.is_ok());
        assert_eq!(History::recent(&history, 1).await.ok(), Some(vec![id]));
    }
}
