# Roadmap

Sequenced thin slices, each one shippable on its own. No slice begins until the
previous one has its `bench/` entry and its integration test green.

## Slice 1 — Memory (IN FLIGHT — core functionality landed)

**Goal:** The best persistent-memory MCP server for Claude Code. Single backend,
honest performance numbers, four tools, maximally flexible.

### Status

- [x] SQLite storage with WAL + content-hash dedup (BLAKE3)
- [x] Schema versioning + forward-only migrations (v1 + v2 shipped)
- [x] FTS5 lexical search (porter unicode61, BM25 ranking)
- [x] Vector search (brute-force cosine, BYO embeddings)
- [x] Hybrid search via Reciprocal Rank Fusion (default mode)
- [x] First-class tags (filter alongside namespace)
- [x] Pluggable `Embedder` trait — defaults to no embedder, ML model optional
- [x] NAPI-rs bindings — `Memory` class with `store`/`get`/`delete`/`stats`/`search`
- [x] Node MCP server registering 4 tools over stdio
- [x] Rust unit tests for round-trip, upsert, delete, lexical, vector, tag filter
- [ ] `bench/memory_*.rs` benchmark files (planned; numbers go in `bench/results/`)
- [ ] End-to-end integration test (Claude Code → MCP → SQLite → restart → recall)
- [ ] CI: `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`

### Done when

- All checkboxes above are green
- README "Status" section updated with measured benchmark numbers
- A Claude Code session can `memory.store`, restart, `memory.search`, and get
  back what it stored — verified end-to-end

---

## Slice 2 — Hooks (IN FLIGHT — SessionStart landed)

**Goal:** Port Ruflo's hook handler honestly. Keep the lifecycle ones that
actually fire and do work. Drop the marketing-tier hooks.

### Status

- [x] `SessionStart` hook — injects N most-recent memories for the current
  project's namespace as session context via Claude Code's
  `hookSpecificOutput.additionalContext` protocol
- [x] Cross-platform hook handler (Node, defensive: always exits 0, 4.5s soft
  timeout, errors to stderr only)
- [x] Idempotent installer with `--scope project|user` and `--dry-run`
- [x] Memory store `recent(namespace, limit)` API — listed without search
- [x] Per-project namespace derived from `<basename>-<sha256[:6]>` of the
  project directory (with `REAL_RUFLO_NAMESPACE` override)
- [x] 2 new unit tests in `crates/memory` (recent ordering + namespace filter)
- [x] End-to-end smoke test through Node verified
- [ ] `Stop` / session-end hook — consolidate notable events from the session
- [ ] `PreToolUse` / `PostToolUse` hooks — log destructive command warnings,
  capture tool outcomes for future search
- [ ] `bench/hooks` measuring per-hook cold-start overhead (target: <200ms p99)

### Done when

- All listed hooks implemented or explicitly deferred to a later slice
- `bench/hooks` produces measured numbers
- Integration test exercises a full session lifecycle through Claude Code

---

## Slice 3 — Orchestrate (IN FLIGHT — core landed)

**Goal:** Turn Ruflo's "memory-as-bus, lead-orchestrated phases" workaround
manual into a first-class declarative API.

### Status

- [x] `phases.yaml` schema: per-phase agent + inputs + outputs + dependencies,
  with parallel groups, output hints, optional descriptions
- [x] YAML parser with full validation: unique phase IDs, blockedBy must exist,
  no self-loops, no cycles (Kahn topological sort), no duplicate output keys
- [x] Phase status derivation from memory: done / partial / ready / blocked
  states computed from which output keys exist in the workflow's namespace
- [x] Markdown brief generator with current input-value previews, sibling
  warnings for parallel agents, output hints, and the verbatim degraded-mode
  paragraph (the load-bearing reality check about subagent limits)
- [x] NAPI bindings: `Orchestrator` class with `validate` / `status` / `brief`
- [x] MCP tools registered: `orchestrate.validate`, `orchestrate.status`,
  `orchestrate.brief`
- [x] Example workflow at `examples/phases-codebase-audit.yaml`
- [x] 13 unit tests in `crates/orchestrate` (schema validation, state
  derivation, brief generation)
- [x] End-to-end smoke test through NAPI verified (parse → status → seed
  memory → re-status → generate brief with live values)
- [ ] `orchestrate.next` — return ALL ready briefs at once for convenience
- [ ] `bench/orchestrate_*.rs` — measure parse + status + brief overhead

### Done when

- All listed boxes above are green
- A real workflow runs end-to-end with lead spawning subagents based on
  generated briefs and advancing via memory verification

---

## Slice 4 — CRDT cross-session replication (post-MVP, may defer)

**Goal:** Multiple Claude sessions (and possibly multiple machines) writing to
the same memory namespace converge without conflicts.

This is the *one* genuine distributed-systems primitive that fits the use case.
Implement only after slices 1-3 are proven. Likely uses `automerge` or
`yrs` rather than rolling our own.

---

## Slice 5+ — Maybe never

These are tracked here so the temptation to add them is visible and resistible.
None of them ship unless there is a clear user need and a clean implementation
path:

- Hive-mind consensus (drop unless someone proposes a deterministic, testable design)
- Claims-based authorization (drop unless it stops being LLM-judgment)
- Neural pattern learning (only with a model card + training data + benchmark)
- WASM-sandboxed agents (interesting but not load-bearing)
- 100-agent mesh (no — fanout from a lead is sufficient)
- A bundled embedding model (BYO is more flexible; keep it that way)

---

## Out of scope (forever, by design)

- Marketing-grade performance numbers without reproducible benchmarks
- LLM-as-policy-engine for security-relevant decisions
- More than 50 MCP tools total
- Two code paths for the same concern
- A second hash function. A second storage backend. A second router. One of each.
