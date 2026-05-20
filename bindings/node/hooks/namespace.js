// Derive the memory namespace for a project from its directory path.
//
// We use the basename of the project directory by default — short, human
// readable, and stable for a given project. Override via the
// REAL_RUFLO_NAMESPACE env var when collisions matter.

"use strict";

const path = require("path");
const crypto = require("crypto");

function projectNamespace(cwd) {
  if (process.env.REAL_RUFLO_NAMESPACE) {
    return process.env.REAL_RUFLO_NAMESPACE;
  }
  const base = path.basename(cwd || process.cwd());
  // Short content-hash suffix to disambiguate same-named projects in
  // different paths (e.g. two projects called "tools").
  const suffix = crypto
    .createHash("sha256")
    .update(cwd || process.cwd())
    .digest("hex")
    .slice(0, 6);
  return `${base}-${suffix}`;
}

module.exports = { projectNamespace };
