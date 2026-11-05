# Release benchmark harness

Run on Windows with:

```powershell
$env:BENCHMARK_TIMESTAMP = Get-Date -Format "yyyy-MM-dd HH:mm:ss zzz"
$env:BENCHMARKS_OUTPUT = "BENCHMARKS.md"
cargo bench -p app --bench release_hardening
```

The harness measures ten cold index-startup-to-first-result samples and 10,000
immediate searches over a synthetic 10,000-entry index. It reports p50/p95
startup and p50/p95/p99 query latency, plus the benchmark process working set
after an idle pause and after the search load. The memory figures are explicitly
not representative of the GUI process idle footprint.
