# 2026-05-20 — Initial build session

The first day on Real Ruflo: audited the original Ruflo, decided to rewrite the
honest parts, and shipped slices 1 and 2.

## Origin

User opened a session in `C:\Users\somet\ruflo` (an active Ruflo install) and
asked how to use Ruflo with Claude Code. They also asked why the Ruflo UI was
showing a dollar cost despite their Claude Max subscription.

### The dollar-cost question, settled

Verified Ruflo is plugged into Claude Code as an MCP server. The dollar number
is a **cost estimate**, not an actual charge:

- No `ANTHROPIC_API_KEY` in the environment
- No `.env` files in the project
- Ruflo's internal config store has zero `anthropic.*` or `api.*` entries
- `hooks_model-stats` reports zero routing decisions made
- Only API key in env is `OLLAMA_API_KEY=local-no...` (local Ollama placeholder)

The Ruflo UI takes the token counts Claude Code reports and multiplies by the
published Anthropic API rates. Useful as a relative gauge, not a bill.

## The audit

User asked for an honest audit. Spawned two parallel research agents:

- **Code audit** — read the local Ruflo install, mapped marketing claims to
  actual implementations, scored each as REAL / PARTIAL / MARKETING / UNKNOWN
- **External reviews** — sandbox blocked all web access; the lead session ran
  the GitHub/npm/web queries directly to backfill

### Honesty score: 4/10

**Real and good:**

- Hook plumbing (`hook-handler.cjs`) — mature, defensive code with real bug history
- Persistent memory loop (PageRank + Jaccard-trigram + FNV-1a dedup) in `intelligence.cjs`
- Native ML binaries actually shipped (`@ruvector/sona`, `attention`, `ruvllm`, ONNX, FlatBuffers)
- Cross-platform Windows hook installer
- `init --wizard` UX
- SPARC methodology skills as recipe templates

**Marketing-grade (the gap):**

- Hive-mind consensus (PBFT/Raft/CRDT/gossip/quorum) — all Markdown LLM prompts. An LLM cannot provide Byzantine fault tolerance.
- Claims-based authorization — 209-line prompt telling an LLM to "evaluate" requests
- 100-agent peer mesh — caps at 8-15 in practice, mesh has no real coordinator
- Issue [#2028](https://github.com/ruvnet/ruflo/issues/2028) (closed) confirmed:
  subagent↔subagent SendMessage silently fails; the project's own CLAUDE.md is
  the workaround manual ("memory-as-bus, lead-orchestrated phases")
- SONA runtime — binaries present but the live learning loop never calls them
- ~250 MCP tools is inflated ~3× by CRUD splits

### External signal

- `ruvnet/ruflo`: **53,305 stars, 6,040 forks, 556 open issues** (created June 2025)
- `claude-flow` npm: **93,785 monthly downloads**
- Bus factor: 1 (ruvnet dominates commits)
- Sentiment: mixed, trending skeptical among engineers who try the advertised
  swarm/neural features in production

## The decision

Build a real version. Personal tool, MIT-licensed, hosted at
[github.com/SRagan/real-ruflo](https://github.com/SRagan/real-ruflo).

**Design principles:**

1. Underclaim, overdeliver — every README claim has a passing test
2. No LLMs where determinism is required (consensus, authorization, scheduling)
3. One backend per concern
4. Design for Claude Code's actual subagent constraints
5. Small tool surface (target ~30 MCP tools, not 250)
6. Performance claims need a `bench/` directory
7. BYO embeddings — no vendor lock

## What we built

### Stack

Rust core (memory, search, embeddings) → NAPI-rs bindings → thin Node MCP
server. Chose this over TS-on-Deno-on-Node-on-WASM to avoid the module-system
mishmash that broke Ruflo's local build
([issue #108](https://github.com/ruvnet/ruflo/issues/108): 149+ TS errors).

### Slice 1 — Memory (done)

- SQLite + FTS5 + WAL + content-hash dedup (BLAKE3)
- Schema v1 + v2 with forward-only migrations (exercised, not aspirational)
- **Vector search**: brute-force cosine, BYO embeddings via `Embedder` trait
- **Lexical search**: FTS5 with BM25 ranking
- **Hybrid search**: Reciprocal Rank Fusion (default), graceful fallback
- First-class tags alongside namespaces
- NAPI `Memory` class: `store`/`get`/`delete`/`stats`/`search`/`recent`
- Node MCP server registering 4 tools (store, search, delete, stats)

Flexibility wins baked in:

- **BYO embeddings** — users supply pre-computed embeddings from any source
  (OpenAI, Anthropic, Cohere, local ONNX); not locked to one vendor
- **Three search modes** — `vector` / `lexical` / `hybrid`, with hybrid as
  default and graceful fallback when one mode lacks input
- **RRF fusion** — no weight knobs, provably better than either mode alone

### CI

GitHub Actions matrix on **Ubuntu + Windows + macOS**:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --all-targets`
- Separate per-platform job builds the NAPI `.node` file

**First CI run: green across all three platforms in 4m40s.**

### Slice 2 — Hooks (SessionStart landed)

- **SessionStart hook** — at every Claude Code session boot, the hook reads
  the top N most-recent entries for the current project's namespace and
  silently injects them as session context (via `hookSpecificOutput.additionalContext`)
- Per-project namespace derived from `<basename>-<sha256[:6]>` of the project
  directory (overridable via `REAL_RUFLO_NAMESPACE`)
- Hook handler: defensive (always exits 0, 4.5s soft timeout, errors stderr-only)
- Idempotent installer with `--scope project|user` and `--dry-run`
- `Stop` / `PostToolUse` / others are stubs ready to fill in

End-to-end smoke tested: seeded memory across two namespaces, ran SessionStart,
observed correct JSON output with namespace filtering working.

## Token-efficiency story

The key value prop the user surfaced themselves: **at scale, search-based
retrieval is dramatically more token-efficient than always-loaded context.**

For 5,000 stored notes:
- Dump all into CLAUDE.md: ~1,000,000 tokens (doesn't fit in 200K window)
- Auto-memory index: ~30,000 tokens for index alone
- Real Ruflo search: ~2,000 tokens (top 10 results × ~200 each) — **99.8% reduction**

Three benefits: token cost, context window space, attention quality.

This is essentially focused, honest, plug-and-play RAG for Claude Code memory.

## How Real Ruflo relates to existing Claude Code memory systems

Three layers, not competitors:

| | CLAUDE.md | Auto-memory | Real Ruflo |
|---|---|---|---|
| Loads | Always | Index always, files on demand | Only what search returns |
| Cost/turn | Whole file | Index | Zero until queried |
| Scale | Hundreds of lines | Dozens of files | Hundreds of thousands of entries |
| Find by | Visual scan | Filename | Meaning |
| Best for | Rules, constitution | Sticky personal facts | Bulk knowledge that grows |

Use all three.

## Toolchain that got installed today

| Tool | Version | Source | Size |
|---|---|---|---|
| GitHub CLI | 2.92.0 | winget | ~80 MB |
| Visual Studio 2022 Build Tools (MSVC C++ workload) | latest | winget | ~5 GB |
| Rust (rustup + stable toolchain) | 1.95.0 | winget | ~1 GB |

Node 24.15.0 + npm 11.12.1 were already installed.

## Repo state at end of session

- https://github.com/SRagan/real-ruflo (public, MIT)
- 4 commits on `main`
- 14 unit tests passing, clippy clean
- CI green
- Working NAPI module: `real-ruflo.win32-x64-msvc.node` (2.2 MB)
- One lifecycle hook live (SessionStart)
- ~1,700 lines of Rust + JS + docs combined

## What's next

- **Slice 1 finish**: `bench/memory_*.rs` benchmarks for the README numbers
- **Slice 2 finish**: `Stop`, `PostToolUse`, `PreCompact` hooks + `bench/hooks_*.rs`
- **Slice 3 (next session — starting now)**: orchestrate. A first-class
  declarative phase runner for multi-agent workflows. Turn Ruflo's
  "memory-as-bus, lead-orchestrated phases" workaround manual into a YAML +
  validator + brief generator + MCP tools.
- Slice 4 (post-MVP): CRDT cross-session replication

## Notes for the next session

- Rust binary path: `C:\Users\somet\.cargo\bin\cargo.exe` (not on shell PATH yet — open a new terminal to pick it up)
- gh CLI path: `C:\Program Files\GitHub CLI\gh.exe` (same — new terminal or full path)
- The hook is installable via `node bindings/node/hooks/install.js --scope project` from inside `bindings/node/`
- Test database location during development: `~/.real-ruflo/memory.db` is the default; `REAL_RUFLO_DB` env var overrides
- Idempotent: re-running the installer removes prior Real Ruflo entries before adding new ones
