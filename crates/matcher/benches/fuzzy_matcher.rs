//! Benchmarks for the `matcher` crate fuzzy scoring engine.

use core::{AppEntry, AppSource, Matcher};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use matcher::{FuzzyMatcher, parallel_search};

/// Generates a synthetic dataset of application entries for benchmarking.
fn generate_benchmark_entries(count: usize) -> Vec<AppEntry> {
    let mut entries = Vec::with_capacity(count);
    let sample_names = [
        "Google Chrome",
        "Mozilla Firefox",
        "Visual Studio Code",
        "Slack",
        "Spotify",
        "Discord",
        "Terminal",
        "Notepad++",
        "Calculator",
        "VLC Media Player",
        "Microsoft Word",
        "Microsoft Excel",
        "Adobe Photoshop",
        "Git Bash",
        "Steam",
        "Docker Desktop",
        "Postman",
        "Sublime Text",
        "Obsidian",
        "IntelliJ IDEA Ultimate Edition",
    ];

    for i in 0..count {
        let base_name = sample_names[i % sample_names.len()];
        let name = if i < sample_names.len() {
            base_name.to_string()
        } else {
            format!("{base_name} {i}")
        };
        let path = format!("C:\\Program Files\\App{i}\\app.exe");
        entries.push(AppEntry::new(name, path, AppSource::StartMenu));
    }

    entries
}

fn bench_fuzzy_search(c: &mut Criterion) {
    let entries = generate_benchmark_entries(10_000);
    let matcher = FuzzyMatcher::new();

    let mut group = c.benchmark_group("fuzzy_matcher");

    // Primary benchmark requirement: 10,000 entries, query "chr"
    group.bench_function("search_10k_query_chr", |b| {
        b.iter(|| {
            let results = matcher.search(black_box("chr"), black_box(&entries));
            black_box(results)
        });
    });

    group.bench_function("search_10k_query_code", |b| {
        b.iter(|| {
            let results = matcher.search(black_box("code"), black_box(&entries));
            black_box(results)
        });
    });

    group.bench_function("parallel_search_10k_query_chr", |b| {
        b.iter(|| {
            let results = parallel_search(black_box(&entries), black_box("chr"), black_box(4));
            black_box(results)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_fuzzy_search);
criterion_main!(benches);
