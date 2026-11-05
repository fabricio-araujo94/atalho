# Release benchmark report

Generated: 2026-11-05 16:35:02 -03:00

Host: `x86_64` (`windows`)

Synthetic index: 10000 application entries; search samples: 10000; startup samples: 10.

| Measurement | Result |
|---|---:|
| Atalho.exe release size (local Windows x64 build) | 8.44 MiB (8,853,504 bytes) |
| Cold start to first searchable response (median) | 42 ms |
| Cold start p95 | 150 ms |
| Search latency p50 | 18553 μs |
| Search latency p95 | 45720 μs |
| Search latency p99 | 100160 μs |
| Benchmark-process working set, idle | 17.1 MiB |
| Benchmark-process working set, after load | 13.7 MiB |

Memory values describe this benchmark process with a synthetic 10k-entry index; they are not a measurement of the interactive launcher's idle footprint. The benchmark captured working set after a one-second quiet period and after all searches. The 40 MB GUI idle-memory target remains unverified. The release executable exceeds the 5 MB target; size attribution is collected by `cargo bloat` in CI.
