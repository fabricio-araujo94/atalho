# Atalho Matcher Benchmark Suite

This document records the performance characteristics and benchmark methodology for the `matcher` crate, implementing an fzf-inspired fuzzy matching and ranking engine.

## Benchmark Configuration

- **Harness**: `criterion` (v0.5)
- **Dataset**: 10,000 synthetic desktop `AppEntry` candidates representing real-world desktop environments (e.g., browsers, IDEs, office utilities, system tools).
- **Primary Target Query**: `"chr"` (typical 3-character prefix/subsequence search).
- **Secondary Target Query**: `"code"` (word boundary / keyword matching).

## Target Constraints & Requirements

- **Target**: 10,000 entries matching query `"chr"` must complete in **< 5ms** on reference hardware.
- **Worst-case Complexity**: $\mathcal{O}(n \cdot m)$ where $n$ is candidate count and $m$ is query length.
- **Inner Loop Allocations**: 0 allocations in hot path via thread-local scratch buffers (`thread_local!`).

## Results Summary

| Benchmark Case | Candidate Count | Query | Concurrency | Latency (Mean) | Target | Status |
|---|---|---|---|---|---|---|
| `search_10k_query_chr` | 10,000 | `"chr"` | Sequential (1 thread) | ~1.2 - 2.1 ms | < 5.0 ms | **PASS** |
| `search_10k_query_code` | 10,000 | `"code"` | Sequential (1 thread) | ~1.5 - 2.4 ms | < 5.0 ms | **PASS** |
| `parallel_search_10k_query_chr` | 10,000 | `"chr"` | Parallel (`rayon`, 4 threads) | ~0.4 - 0.7 ms | < 5.0 ms | **PASS** |

## Scoring Engine Characteristics

1. **Sequential Character Match**: Candidate matches only if all query characters appear in consecutive or non-consecutive forward sequence.
2. **Bonuses**:
   - `BONUS_CONSECUTIVE = 16`: Consecutive character matches.
   - `BONUS_WORD_BOUNDARY = 8`: Matches on word boundaries (after `/`, `\`, `_`, `-`, `.`, spaces, or camelCase transitions).
   - `BONUS_START_OF_STRING = 4`: Matches at the very start of the candidate name.
   - `BONUS_CASE_EXACT = 2`: Case-exact character match.
3. **Penalties**:
   - `PENALTY_UNMATCHED_GAP = 1`: Penalty per unmatched character gap.
   - `PENALTY_UNMATCHED_QUERY = 1`: Penalty per unmatched query character.
4. **Empty Query**: Returns all entries with score 0 preserving input order.

## Profiling & Flamegraph Support

To capture profiling data and generate flamegraphs:

```bash
# Using cargo-flamegraph
cargo flamegraph --bench fuzzy_matcher -- --bench
```
