# Real Ruflo hooks

Claude Code lifecycle hooks that make Real Ruflo's memory automatically useful.

## What's implemented

| Hook            | Status   | Behavior                                                          |
|-----------------|----------|-------------------------------------------------------------------|
| `SessionStart`  | working  | Injects the top N recent memories for the current project's namespace as session context |
| `SessionEnd`    | stub     | No-op (planned: consolidate notable events from the session)      |
| `PreToolUse`    | stub     | No-op (planned: log destructive command warnings)                 |
| `PostToolUse`   | stub     | No-op (planned: capture tool outcomes for future search)          |
| `Stop`          | stub     | No-op                                                              |

Stubs exit 0 immediately, so installing them costs nothing until the
implementations land.

## Install

```bash
# From the bindings/node directory:
node hooks/install.js                 # writes to ./.claude/settings.json (project)
node hooks/install.js --scope user    # writes to ~/.claude/settings.json (global)
node hooks/install.js --dry-run       # preview without writing
```

The installer is **idempotent**: running it again removes any prior
`real-ruflo` hook entries before re-installing, so re-installing never
duplicates.

The installer is **safe**: it reads your existing settings.json, merges
in our hook entries alongside any other hooks you have, and writes back
atomically. It will refuse to run if the existing file is malformed JSON.

## Config knobs (environment variables)

| Var                                | Default                   | Meaning                                              |
|------------------------------------|---------------------------|------------------------------------------------------|
| `REAL_RUFLO_DB`                    | `~/.real-ruflo/memory.db` | Path to the memory database                          |
| `REAL_RUFLO_NAMESPACE`             | `<basename>-<sha256[:6]>` | Override the per-project namespace                   |
| `REAL_RUFLO_SESSION_CONTEXT_LIMIT` | `5`                       | How many recent entries to inject at session start   |
| `REAL_RUFLO_HOOK_TIMEOUT_MS`       | `4500`                    | Internal soft timeout (Claude Code's hook limit is 5s) |

## Defensive contract

Every hook:

- **Always exits 0**, even on internal errors. A failed hook never blocks Claude Code.
- **Hard timeout** at `REAL_RUFLO_HOOK_TIMEOUT_MS` (default 4.5s). The process self-terminates rather than hang.
- **Silent in normal operation** — only writes to stdout if the hook protocol expects structured output.
- **Errors go to stderr**, not stdout, so they don't corrupt Claude Code's context injection.
