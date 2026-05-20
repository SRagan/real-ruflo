# Memory subsystem — design

This is the design doc for ROADMAP slice 1. Implementation lives in
`crates/memory/`. Done-criteria are in `ROADMAP.md`.

## Goal

A boring, correct, fast persistent memory store that Claude Code sessions can
write to and read from across restarts. One backend. Four tools.

## What it stores

Key-value pairs scoped to a namespace. Value is arbitrary JSON. Each entry has
a content hash (BLAKE3 of whitespace-normalized serialization) so that
duplicate writes are visible as duplicates and can be deduped if we choose.

```
(namespace, key) -> { value, content_hash, created_at, accessed_at, access_count, embedding? }
```

## What's deliberately not in v0

- **Embeddings:** wired but optional. The store works without them; search
  degrades to lexical FTS5 until embeddings catch up. We don't block writes on
  the embedding pipeline.
- **PageRank-style relevance boosting:** Ruflo does this. It's clever but
  unnecessary for the MVP. Land plain vector + lexical first, see if anything
  is missing, then maybe add it.
- **TTL-based eviction:** add when we observe a real need.
- **CRDT replication:** slice 4.
- **Compression:** SQLite handles this fine via WAL; no custom compression.

## Decision: SQLite + `sqlite-vec` (no custom HNSW)

`sqlite-vec` is a recent extension that gives us:

- A virtual table for vectors stored inline
- Brute-force cosine k-NN (no index) — fast enough for ~100k entries
- Optional `vec0` indexed mode for larger collections (we don't need it yet)
- Loaded as a SQLite extension; works from `rusqlite` with feature `load_extension`

We choose this over a hand-rolled HNSW because:

- We don't have the expertise to maintain a production HNSW
- 100k entries is plenty for Claude Code session memory
- Brute-force at 100k vectors is ~10ms on modern hardware — fast enough
- We can swap to HNSW later if `bench/memory_search.rs` shows we need it

We **do not** claim "sub-millisecond HNSW" until `bench/` proves it.

## Decision: BLAKE3 over FNV-1a for content hashing

Ruflo uses FNV-1a 64-bit. BLAKE3 is:

- Cryptographically secure (defends against adversarial collision attacks if
  someone ever stores untrusted content)
- Fast (faster than SHA-256, comparable to FNV in our size regime)
- 256-bit so collision probability is negligible at any practical scale
- Already in our dependency graph

The tradeoff (slightly larger hash storage) is irrelevant at SQLite scale.

## Decision: WAL mode + NORMAL synchronous

- WAL mode lets concurrent readers proceed without blocking writers
- `synchronous = NORMAL` is the right tradeoff for a developer tool (vs.
  `FULL`'s extra fsync cost). We accept the theoretical risk of a recent
  write being lost on OS crash; we are not a transactional system of record.

## Tool surface (MCP)

| Tool             | Args                                | Returns                       |
|------------------|-------------------------------------|-------------------------------|
| `memory.store`   | `namespace`, `key`, `value`         | `{ ok: true }`                |
| `memory.search`  | `query`, `namespace?`, `limit?`     | `[{ entry, score }, ...]`     |
| `memory.delete`  | `namespace`, `key`                  | `{ deleted: boolean }`        |
| `memory.stats`   | (none)                              | `{ total_entries, namespaces }` |

That's the whole surface. We don't need `_unified`, `_compress`, `_migrate`,
`_export`, `_import`, `_bridge_status`, `_detailed-stats`, `_list`. Those are
all internal concerns or one-shot scripts.

## Open questions for the implementation

1. **Embedding model selection.** Default candidate: `all-MiniLM-L6-v2`
   (22 MB, 384-dim). Alternative: `bge-small-en-v1.5` (33 MB, 384-dim,
   slightly better retrieval). Both run in `ort` on CPU at <50ms/text.
2. **How to bundle the model.** Options: (a) bundle in the binary (~30 MB
   binary inflate), (b) download on first run with checksum verification.
   Leaning toward (b) — keeps the cdylib small and lets users opt out.
3. **What does Claude Code's MCP server actually need re. concurrency?**
   We can assume one connection per session in v0; verify before slice 2.
