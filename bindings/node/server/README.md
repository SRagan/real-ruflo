# Real Ruflo MCP server

Stdio MCP server exposing the four memory tools to Claude Code.

## Build the native module

```bash
cd bindings/node
npm install
npm run build           # release; debug build via `npm run build:debug`
```

This produces a platform-specific `.node` file (e.g. `real-ruflo.win32-x64-msvc.node`)
in `bindings/node/`.

## Wire into Claude Code

Add to your `.mcp.json` (or `~/.claude.json`):

```json
{
  "mcpServers": {
    "real-ruflo": {
      "command": "node",
      "args": [
        "C:\\absolute\\path\\to\\Real Ruflo\\bindings\\node\\server\\index.js"
      ],
      "env": {
        "REAL_RUFLO_DB": "C:\\Users\\you\\.real-ruflo\\memory.db"
      }
    }
  }
}
```

`REAL_RUFLO_DB` is optional. Default is `~/.real-ruflo/memory.db` (or
`%USERPROFILE%\.real-ruflo\memory.db` on Windows).

## Tools registered

| Tool             | Args                                                          |
|------------------|---------------------------------------------------------------|
| `memory.store`   | `namespace`, `key`, `value`, `tags?`, `embedding?`            |
| `memory.search`  | `query`, `embedding?`, `namespace?`, `tags?`, `limit?`, `mode?` |
| `memory.delete`  | `namespace`, `key`                                            |
| `memory.stats`   | (none)                                                         |

`mode` accepts `vector`, `lexical`, or `hybrid` (default).

## BYO embeddings

You don't need an ML model on disk to use vector search. Compute embeddings
upstream (OpenAI `text-embedding-3-small`, Anthropic, Cohere, a local
`ort`+ONNX model, anything) and pass them via the `embedding` field on
`memory.store` and `memory.search`. The store treats all sources identically
as long as dimensionalities match within a namespace.
