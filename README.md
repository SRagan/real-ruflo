# Real Ruflo

A grounded, honest reimagining of [ruvnet/ruflo](https://github.com/ruvnet/ruflo).
Personal tool, MIT-licensed, built to do exactly what the README says it does — no more.

## Why this exists

Ruflo ships a lot of theatre. Hive-mind Byzantine consensus implemented as Markdown
prompts. Claims-based authorization as LLM judgment. A "100-agent swarm" that caps
at 8 in practice and whose subagents can't talk to each other (issue
[#2028](https://github.com/ruvnet/ruflo/issues/2028)). Real Ruflo throws all of that
out and keeps only what works:

- Claude Code lifecycle hooks
- Persistent semantic memory across sessions
- A first-class lead-orchestrated phase runner for multi-agent work

Anything that can't pass a benchmark in `bench/` doesn't ship.

## Status

**Pre-alpha — slice 1 (memory) done; slice 2 (hooks) in flight.**

| Subsystem      | Status                                              |
|----------------|-----------------------------------------------------|
| Memory store   | working — SQLite + WAL + FTS5 + tags + content-hash |
| Vector search  | working — brute-force cosine, BYO embeddings        |
| Lexical search | working — FTS5 with BM25                            |
| Hybrid search  | working — Reciprocal Rank Fusion, default mode      |
| Recent listing | working — list most-recently-accessed by namespace  |
| NAPI bindings  | working — `Memory` class, 6 methods                 |
| MCP server     | working — stdio, 4 tools registered                 |
| Hooks (SessionStart) | working — auto-injects project memories on session boot |
| Hook installer | working — idempotent, project- or user-scoped       |
| Tests          | 14 unit tests passing in `crates/memory`            |
| CI             | working — GitHub Actions on Ubuntu/Windows/macOS    |
| Benchmarks     | TODO — see `bench/README.md`                        |

## Design principles

1. **Underclaim, overdeliver.** Every README claim has a passing test.
2. **No LLMs where determinism is required.** Consensus, authorization,
   scheduling → real code. Ranking, synthesis, classification → LLMs fine.
3. **One backend per concern.** One memory store. One router. One hook engine.
4. **Design for Claude Code's actual constraints.** Subagents are stateless
   one-shots. The architecture treats this as a feature, not a bug.
5. **Small tool surface.** Target ~30 MCP tools total. CRUD doesn't get
   counted three times.
6. **Performance claims need a `bench/` directory.** No number ships without
   a reproducible benchmark.
7. **BYO embeddings.** No bundled ML model; plug in any embedding source —
   OpenAI, Anthropic, Cohere, local ONNX, anything that produces floats.

## Quick start

### Build

```bash
cd "Real Ruflo"
cargo test -p real-ruflo-memory     # should pass — 9 tests
cd bindings/node
npm install
npm run build                       # produces real-ruflo.<platform>.node
```

### Wire into Claude Code

Add to your `.mcp.json` (or `~/.claude.json`):

```json
{
  "mcpServers": {
    "real-ruflo": {
      "command": "node",
      "args": ["<absolute-path>/Real Ruflo/bindings/node/server/index.js"]
    }
  }
}
```

Then in a session:
- `memory.store` — write
- `memory.search` — query with `mode` of `vector` / `lexical` / `hybrid`
- `memory.delete` / `memory.stats` — housekeeping

### Install hooks (optional, for automatic session context)

```bash
cd bindings/node
node hooks/install.js --scope project    # per-project hooks
# or --scope user for global hooks
```

This registers a `SessionStart` hook that injects the top 5 recent memories
for the current project's namespace into Claude Code at boot — no explicit
`memory.search` call needed. See `bindings/node/hooks/README.md` for details
and configuration knobs.

## Architecture

Rust core (memory, search, embeddings) → NAPI-rs bindings → thin Node MCP server.
See [ARCHITECTURE.md](./ARCHITECTURE.md).

## Roadmap

See [ROADMAP.md](./ROADMAP.md). Slice 1 (memory) is in flight. Slices 2 (hooks)
and 3 (orchestrate) are designed but not started.

## License

MIT. See [LICENSE](./LICENSE).
