#!/usr/bin/env node
// Real Ruflo hook handler. Invoked by Claude Code on lifecycle events.
//
// Dispatch:
//   real-ruflo-hooks session-start
//   real-ruflo-hooks session-end
//   real-ruflo-hooks pre-edit | post-edit
//   real-ruflo-hooks pre-task  | post-task
//   real-ruflo-hooks pre-command | post-command
//
// Defensive contract:
//   - Always exits 0, even on internal errors. Hooks must never block
//     Claude Code or surface unhandled exceptions.
//   - Hard timeout via setTimeout(... 4500). Claude Code's default is 5s.
//   - All errors go to stderr; nothing leaks via stdout unless it's a
//     well-formed JSON response Claude Code expects.

"use strict";

const fs = require("fs");
const path = require("path");

const { projectNamespace } = require("./namespace.js");

const HOOK = process.argv[2] || "noop";
const CWD = process.env.CLAUDE_PROJECT_DIR || process.cwd();
const NS = projectNamespace(CWD);

const TIMEOUT_MS = parseInt(process.env.REAL_RUFLO_HOOK_TIMEOUT_MS || "4500", 10);
const timer = setTimeout(() => {
  // Time's up — exit cleanly so we don't block Claude Code.
  process.exit(0);
}, TIMEOUT_MS);
timer.unref();

async function main() {
  switch (HOOK) {
    case "session-start":
      return await sessionStart();
    case "session-end":
    case "pre-edit":
    case "post-edit":
    case "pre-task":
    case "post-task":
    case "pre-command":
    case "post-command":
      // Stubs — implemented in follow-up commits. Exit 0 silently.
      return;
    default:
      // Unknown hook: don't crash Claude Code, just no-op.
      return;
  }
}

async function sessionStart() {
  let memory;
  try {
    const mod = require("../index.js");
    memory = new mod.Memory(process.env.REAL_RUFLO_DB || null);
  } catch (err) {
    // Native module not built or DB unavailable. Silent no-op.
    process.stderr.write(`real-ruflo: session-start skipped: ${err.message}\n`);
    return;
  }

  const limit = parseInt(process.env.REAL_RUFLO_SESSION_CONTEXT_LIMIT || "5", 10);
  let entries;
  try {
    entries = memory.recent(NS, limit);
  } catch (err) {
    process.stderr.write(`real-ruflo: session-start recent() failed: ${err.message}\n`);
    return;
  }

  if (!Array.isArray(entries) || entries.length === 0) {
    // Nothing to inject. Still exit 0 — silence is correct here.
    return;
  }

  const lines = entries.map((entry) => {
    const key = entry.key;
    const value = stringifyValue(entry.value);
    const ago = humanAgo(entry.accessed_at);
    return `- **${key}** (${ago}) — ${value}`;
  });

  const block =
    `## Real Ruflo memory for \`${NS}\`\n\n` +
    `${lines.join("\n")}\n\n` +
    `_Call \`memory.search\` for more, \`memory.store\` to add new entries._\n`;

  // Claude Code's SessionStart hook protocol: write JSON to stdout with
  // an `additionalContext` field. See:
  //   https://docs.anthropic.com/en/docs/claude-code/hooks
  process.stdout.write(
    JSON.stringify({
      hookSpecificOutput: {
        hookEventName: "SessionStart",
        additionalContext: block,
      },
    })
  );
}

function stringifyValue(value, maxLen = 200) {
  let s;
  if (typeof value === "string") {
    s = value;
  } else {
    try {
      s = JSON.stringify(value);
    } catch {
      s = String(value);
    }
  }
  s = s.replace(/\s+/g, " ").trim();
  return s.length > maxLen ? s.slice(0, maxLen - 1) + "…" : s;
}

function humanAgo(timestampMs) {
  if (!timestampMs) return "unknown";
  const seconds = Math.max(0, Math.floor((Date.now() - timestampMs) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

main().catch((err) => {
  process.stderr.write(`real-ruflo: ${HOOK} error: ${err.message}\n`);
  process.exit(0);
});

// In case main() doesn't return promptly, ensure we exit.
process.on("exit", () => clearTimeout(timer));
