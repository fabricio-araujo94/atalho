//! End-to-end startup, search-latency, and process-memory measurements.

use app::search::SearchService;
use core::{AppEntry, AppSource, Matcher, Persistence, SearchRequest};
use indexer::DefaultIndexer;
use matcher::FuzzyMatcher;
use persistence::InMemoryPersistence;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

const ENTRY_COUNT: usize = 10_000;
const QUERY_COUNT: usize = 10_000;
const STARTUP_SAMPLES: usize = 10;

fn main() -> Result<(), Box<dyn Error>> {
    let mut startup_samples = measure_startup()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let (service, persistence) = runtime.block_on(create_ready_service())?;
    std::thread::sleep(Duration::from_secs(1));
    let idle_memory = process_memory_bytes();

    let mut latencies_us = Vec::with_capacity(QUERY_COUNT);
    for query_index in 0..QUERY_COUNT {
        let query = match query_index % 4 {
            0 => "bench",
            1 => "app",
            2 => "benchmark",
            _ => "app 99",
        };
        let start = Instant::now();
        let response = runtime.block_on(service.search(SearchRequest::new(query, 8)))?;
        std::hint::black_box(response);
        latencies_us.push(u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX));
    }
    let loaded_memory = process_memory_bytes();
    latencies_us.sort_unstable();
    startup_samples.sort_unstable();

    let report = format!(
        "# Release benchmark report\n\n\
         Generated: {}\n\n\
         Host: `{}` (`{}`)\n\n\
         Synthetic index: {ENTRY_COUNT} application entries; search samples: {QUERY_COUNT}; startup samples: {STARTUP_SAMPLES}.\n\n\
         | Measurement | Result |\n\
         |---|---:|\n\
         | Cold start to first searchable response (median) | {} ms |\n\
         | Cold start p95 | {} ms |\n\
         | Search latency p50 | {} μs |\n\
         | Search latency p95 | {} μs |\n\
         | Search latency p99 | {} μs |\n\
         | Benchmark-process working set, idle | {} |\n\
         | Benchmark-process working set, after load | {} |\n\n\
         Memory values describe this benchmark process with a synthetic 10k-entry index; they are not a measurement of the interactive launcher's idle footprint. The weekly self-hosted run is intended to provide a stable baseline. Release binary size is recorded by the Windows build job.\n",
        chrono_like_timestamp(),
        std::env::consts::ARCH,
        std::env::consts::OS,
        percentile(&startup_samples, 50),
        percentile(&startup_samples, 95),
        percentile(&latencies_us, 50),
        percentile(&latencies_us, 95),
        percentile(&latencies_us, 99),
        format_bytes(idle_memory),
        format_bytes(loaded_memory),
    );
    let default_output_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("BENCHMARKS.md");
    let output_path = std::env::var_os("BENCHMARKS_OUTPUT").map_or(default_output_path, Into::into);
    std::fs::write(&output_path, report)?;
    println!("Wrote benchmark report to {}", output_path.display());

    drop(service);
    drop(persistence);
    runtime.shutdown_timeout(Duration::from_secs(2));
    Ok(())
}

fn measure_startup() -> Result<Vec<u64>, Box<dyn Error>> {
    let mut samples = Vec::with_capacity(STARTUP_SAMPLES);
    for _ in 0..STARTUP_SAMPLES {
        let start = Instant::now();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        let (service, _persistence) = runtime.block_on(create_ready_service())?;
        let response = runtime.block_on(service.search(SearchRequest::new("bench", 8)))?;
        std::hint::black_box(response);
        samples.push(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX));
        service.cancel_tasks();
        runtime.shutdown_timeout(Duration::from_secs(2));
    }
    Ok(samples)
}

async fn create_ready_service()
-> Result<(Arc<SearchService>, Arc<InMemoryPersistence>), Box<dyn Error>> {
    let entries = (0..ENTRY_COUNT)
        .map(|index| {
            AppEntry::new(
                format!("Benchmark App {index:05}"),
                format!("C:\\Benchmark\\App{index:05}.exe"),
                AppSource::StartMenu,
            )
        })
        .collect();
    let indexer = Arc::new(DefaultIndexer::with_entries(entries, Vec::new()));
    let matcher = Arc::new(FuzzyMatcher::new());
    let persistence = Arc::new(InMemoryPersistence::new());
    let persistence_trait: Arc<dyn Persistence> = persistence.clone();
    let matcher_trait: Arc<dyn Matcher> = matcher;
    let service = Arc::new(SearchService::new(
        indexer,
        matcher_trait,
        persistence_trait,
    ));
    service.initialize().await?;

    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if service.cache().read().await.len() == ENTRY_COUNT {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    Ok((service, persistence))
}

fn percentile(sorted_values: &[u64], percentile: u32) -> u64 {
    if sorted_values.is_empty() {
        return 0;
    }
    let len = u64::try_from(sorted_values.len()).unwrap_or(u64::MAX);
    let rank = len.saturating_mul(u64::from(percentile)).div_ceil(100);
    let index = usize::try_from(rank.saturating_sub(1)).unwrap_or(usize::MAX);
    sorted_values[index.min(sorted_values.len() - 1)]
}

fn format_bytes(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || "unavailable".to_owned(),
        |bytes| {
            let mib = 1024 * 1024;
            format!("{}.{:01} MiB", bytes / mib, (bytes % mib) * 10 / mib)
        },
    )
}

fn chrono_like_timestamp() -> String {
    std::env::var("BENCHMARK_TIMESTAMP").unwrap_or_else(|_| "not supplied by runner".to_owned())
}

#[cfg(windows)]
fn process_memory_bytes() -> Option<u64> {
    use std::mem::size_of;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS>()).ok()?,
        ..PROCESS_MEMORY_COUNTERS::default()
    };
    // SAFETY: counters is writable and sized as declared; GetCurrentProcess returns a pseudo-handle.
    unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &raw mut counters, counters.cb) }.ok()?;
    u64::try_from(counters.WorkingSetSize).ok()
}

#[cfg(not(windows))]
fn process_memory_bytes() -> Option<u64> {
    None
}
