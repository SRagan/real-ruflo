# Benchmarks

**Rule:** Every performance claim in the project's README or marketing must have
a corresponding benchmark in this directory that anyone can run and reproduce.
If a number doesn't have a bench file, it doesn't ship.

## How to run

```bash
cargo bench -p real-ruflo-memory
```

Output goes to `bench/results/<datestamp>.md` for the record.

## Planned benches (slice 1)

| File                     | What it measures                                          |
|--------------------------|-----------------------------------------------------------|
| `memory_store.rs`        | Store latency at 1k / 10k / 100k entries                  |
| `memory_search.rs`       | Vector search latency + recall@10 at the same scales      |
| `memory_dedup.rs`        | Hash dedup overhead vs. naive equality                    |
| `memory_concurrent.rs`   | Two concurrent writers via WAL — correctness + latency    |

## What we do NOT do

- We do not cherry-pick a hot-cache run and report it as steady-state.
- We do not report best-of-N — we report median and p95.
- We do not compare to ourselves on different hardware.
- We do not invent units ("12,500x faster" — vs. what?).
- We do not benchmark in isolation what is never used in isolation.
