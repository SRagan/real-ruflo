# Architecture

## Stack

```
┌─────────────────────────────────────────────┐
│  Claude Code (MCP client)                   │
└─────────────────────────────────────────────┘
                    │  stdio JSON-RPC (MCP)
┌─────────────────────────────────────────────┐
│  bindings/node/server/index.js              │
│    • registers memory.store/search/...      │
│    • calls into NAPI module                 │
└─────────────────────────────────────────────┘
                    │  NAPI-rs (sync)
┌─────────────────────────────────────────────┐
│  bindings/node/src/lib.rs                   │
│    • Memory class                           │
│    • StoreArgs / SearchArgs                 │
└─────────────────────────────────────────────┘
                    │
┌─────────────────────────────────────────────┐
│  crates/memory                              │
│    • MemoryStore                            │
│    • Embedder trait (pluggable, BYO)        │
│    • SearchRequest + SearchMode             │
│    • RRF fusion                             │
│    • Schema v1+v2 migrations                │
└─────────────────────────────────────────────┘
                    │
            ~/.real-ruflo/memory.db
            (SQLite + FTS5)
```

## Why Rust + NAPI

- Single static binary for perf-sensitive bits (vector search, hash dedup).
- Node surface for the MCP protocol where iteration speed matters and the SDK
  is mature.
- NAPI-rs is the production path used by SWC, Next.js, Prisma.
- Avoids the Deno/Node/WASM mishmash that broke Ruflo's local build
  ([issue #108](https://github.com/ruvnet/ruflo/issues/108): 149+ TS errors).

## Memory subsystem (slice 1)

### Storage

One SQLite database at `~/.real-ruflo/memory.db`.

Schema v2:
```sql
CREATE TABLE entries (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  namespace    TEXT NOT NULL,
  key          TEXT NOT NULL,
  value        TEXT NOT NULL,            -- JSON-serialized
  tags         TEXT NOT NULL DEFAULT '[]',  -- JSON array
  content_hash BLOB NOT NULL,            -- BLAKE3 of normalized value
  embedding    BLOB,                     -- packed little-endian f32
  embed_dim    INTEGER,
  created_at   INTEGER NOT NULL,
  accessed_at  INTEGER NOT NULL,
  access_count INTEGER NOT NULL DEFAULT 0,
  UNIQUE(namespace, key)
);

CREATE VIRTUAL TABLE entries_fts USING fts5(
  value, tags,
  content='entries', content_rowid='id',
  tokenize='porter unicode61'
);
```

Triggers keep `entries_fts` in sync with `entries` on insert/update/delete.

### Search

Three modes:
- **`vector`** — brute-force cosine over stored f32 BLOBs. Requires both query
  embedding and stored embeddings.
- **`lexical`** — FTS5 with BM25 ranking. Always available.
- **`hybrid` (default)** — runs both, fuses with Reciprocal Rank Fusion
  (k=60). Falls back gracefully:
  - No query embedding → lexical only
  - No stored embeddings → lexical only
  - Both available → fused

Why brute-force cosine, not HNSW or `sqlite-vec`:
- Fewer native deps (no extension to load)
- Fast enough for ~100k entries (verified by future `bench/memory_search.rs`)
- A clean trait boundary means we can swap implementations later if benchmarks
  demand it. No claims like "sub-millisecond HNSW" ship until proven.

### Embeddings: BYO by design

The system is intentionally model-agnostic. Three usage patterns are supported
equally:

1. **External embeddings (recommended for production).** Compute upstream from
   any source — OpenAI `text-embedding-3-small`, Anthropic, Cohere, local
   `ort`+ONNX — and pass via `embedding` field on `memory.store` and
   `memory.search`. The store stores any-dimensional f32 vectors and matches
   them by length.

2. **Plugged-in embedder.** Implement `real_ruflo_memory::Embedder` in Rust
   and pass the impl via `MemoryStore::open_with(path, embedder)`. The store
   calls it automatically when `embedding` is absent.

3. **No embeddings.** Default `NoEmbedder` ships out of the box. Lexical
   search via FTS5 still works.

This is more flexible than bundling a model: users pick their embedding
tradeoff (cost, quality, latency, privacy), and we don't ship 30 MB of model
weights that may not match their needs.

### MCP tool surface

| Tool             | Args                                                              | Returns                       |
|------------------|-------------------------------------------------------------------|-------------------------------|
| `memory.store`   | `namespace`, `key`, `value`, `tags?`, `embedding?`                | `{ ok: true }`                |
| `memory.search`  | `query`, `embedding?`, `namespace?`, `tags?`, `limit?`, `mode?`   | `[{ entry, score, source }]`  |
| `memory.delete`  | `namespace`, `key`                                                | `{ deleted: boolean }`        |
| `memory.stats`   | (none)                                                            | `{ total_entries, namespaces, entries_with_embeddings }` |

Four tools. That's the whole surface for slice 1.

## What we deliberately won't do

- **No "hive-mind" naming.** It's a phase runner, not a hive.
- **No LLM-as-consensus / LLM-as-policy.** Where determinism matters, write
  real code or don't ship the feature.
- **No buzzword inflation.** No "Flash Attention 2.49x-7.47x" unless the
  binary is in the repo and `bench/flash_attention.rs` prints those numbers.
- **No nightly Rust.** Stable only.
- **No schema churn for the user.** Once a schema version ships, it migrates
  forward; it does not break existing DBs.
- **No bundled embedding model.** BYO is more flexible and avoids forcing a
  vendor or model choice.
