#!/usr/bin/env node
// Real Ruflo hooks installer.
//
// Usage:
//   node hooks/install.js [--scope project|user] [--dry-run]
//
// Reads the target settings.json, merges in Real Ruflo hook entries, writes
// back atomically. Never clobbers existing hook entries — appends instead.

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");

const args = parseArgs(process.argv.slice(2));
const SCOPE = args.scope || "project";
const DRY_RUN = args["dry-run"];

const HANDLER = path.resolve(__dirname, "handler.js");

if (!fs.existsSync(HANDLER)) {
  console.error(`real-ruflo: handler not found at ${HANDLER}`);
  process.exit(1);
}

const settingsPath = resolveSettingsPath(SCOPE);
const existing = readJsonSafe(settingsPath);

const ourHooks = buildHooks(HANDLER);
const merged = mergeHooks(existing, ourHooks);

if (DRY_RUN) {
  console.log(`Would write to ${settingsPath}:`);
  console.log(JSON.stringify(merged, null, 2));
  process.exit(0);
}

ensureDir(path.dirname(settingsPath));
writeJsonAtomic(settingsPath, merged);

console.log(`real-ruflo: hooks installed → ${settingsPath}`);
console.log("Restart Claude Code for the hooks to take effect.");

function buildHooks(handlerPath) {
  // Cross-platform invocation: on Windows use `node` directly; the path
  // quoting handles spaces.
  const cmd = `node "${handlerPath}"`;
  return {
    SessionStart: [
      {
        matcher: "",
        hooks: [
          {
            type: "command",
            command: `${cmd} session-start`,
            timeout: 5,
          },
        ],
      },
    ],
  };
}

function mergeHooks(existing, ours) {
  const out = { ...existing };
  out.hooks = { ...(existing.hooks || {}) };

  for (const event of Object.keys(ours)) {
    const existingForEvent = Array.isArray(out.hooks[event]) ? out.hooks[event] : [];
    // Drop any prior Real Ruflo entries so re-install is idempotent.
    const filtered = existingForEvent.filter((entry) => {
      const cmd = (entry && entry.hooks && entry.hooks[0] && entry.hooks[0].command) || "";
      return !cmd.includes("real-ruflo") && !cmd.includes(HANDLER);
    });
    out.hooks[event] = [...filtered, ...ours[event]];
  }
  return out;
}

function resolveSettingsPath(scope) {
  if (scope === "user") {
    return path.join(os.homedir(), ".claude", "settings.json");
  }
  if (scope === "project") {
    return path.join(process.cwd(), ".claude", "settings.json");
  }
  console.error(`real-ruflo: unknown scope: ${scope}`);
  process.exit(2);
}

function readJsonSafe(p) {
  try {
    if (!fs.existsSync(p)) return {};
    const raw = fs.readFileSync(p, "utf8");
    if (!raw.trim()) return {};
    return JSON.parse(raw);
  } catch (err) {
    console.error(`real-ruflo: could not parse ${p}: ${err.message}`);
    console.error("Refusing to overwrite a malformed settings file.");
    process.exit(3);
  }
}

function ensureDir(d) {
  if (!fs.existsSync(d)) fs.mkdirSync(d, { recursive: true });
}

function writeJsonAtomic(p, value) {
  const tmp = `${p}.tmp-${process.pid}`;
  fs.writeFileSync(tmp, JSON.stringify(value, null, 2) + "\n");
  fs.renameSync(tmp, p);
}

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const key = a.slice(2);
      const next = argv[i + 1];
      if (next && !next.startsWith("--")) {
        out[key] = next;
        i++;
      } else {
        out[key] = true;
      }
    }
  }
  return out;
}
