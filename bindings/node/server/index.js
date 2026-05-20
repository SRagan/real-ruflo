#!/usr/bin/env node
// Real Ruflo MCP server — registers memory.store / .search / .delete / .stats
// over stdio. Wired into Claude Code via .mcp.json:
//
//   { "mcpServers": { "real-ruflo": { "command": "node",
//       "args": ["<abs-path>/bindings/node/server/index.js"] } } }

const { Server } = require("@modelcontextprotocol/sdk/server/index.js");
const {
  StdioServerTransport,
} = require("@modelcontextprotocol/sdk/server/stdio.js");
const {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} = require("@modelcontextprotocol/sdk/types.js");

const { Memory, Orchestrator } = require("../index.js");

const memory = new Memory(process.env.REAL_RUFLO_DB || null);
const orchestrator = new Orchestrator(process.env.REAL_RUFLO_DB || null);

const tools = [
  {
    name: "memory.store",
    description:
      "Persist a value under (namespace, key). Optional tags for filtering, " +
      "optional pre-computed embedding from any source.",
    inputSchema: {
      type: "object",
      required: ["namespace", "key", "value"],
      properties: {
        namespace: { type: "string" },
        key: { type: "string" },
        value: {},
        tags: { type: "array", items: { type: "string" } },
        embedding: { type: "array", items: { type: "number" } },
      },
    },
  },
  {
    name: "memory.search",
    description:
      "Search the memory store. Mode 'hybrid' (default) fuses vector + " +
      "lexical via Reciprocal Rank Fusion. Pass `embedding` for vector " +
      "search; without one, falls back to lexical.",
    inputSchema: {
      type: "object",
      required: ["query"],
      properties: {
        query: { type: "string" },
        embedding: { type: "array", items: { type: "number" } },
        namespace: { type: "string" },
        tags: { type: "array", items: { type: "string" } },
        limit: { type: "integer", minimum: 1, maximum: 200 },
        mode: { type: "string", enum: ["vector", "lexical", "hybrid"] },
      },
    },
  },
  {
    name: "memory.delete",
    description: "Delete an entry by (namespace, key). Returns { deleted: bool }.",
    inputSchema: {
      type: "object",
      required: ["namespace", "key"],
      properties: {
        namespace: { type: "string" },
        key: { type: "string" },
      },
    },
  },
  {
    name: "memory.stats",
    description:
      "Get counts: total_entries, namespaces, entries_with_embeddings.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "orchestrate.validate",
    description:
      "Validate a phases.yaml workflow file (DAG, unique IDs, no cycles, " +
      "no duplicate output keys). Returns the parsed workflow.",
    inputSchema: {
      type: "object",
      required: ["workflow_path"],
      properties: {
        workflow_path: { type: "string" },
      },
    },
  },
  {
    name: "orchestrate.status",
    description:
      "Show the current state of every phase in a workflow, derived from " +
      "what's currently in the memory namespace. Each phase is " +
      "done/partial/ready/blocked.",
    inputSchema: {
      type: "object",
      required: ["workflow_path"],
      properties: {
        workflow_path: { type: "string" },
      },
    },
  },
  {
    name: "orchestrate.brief",
    description:
      "Generate a markdown brief for spawning a specific agent in a " +
      "specific phase. Includes input memory keys (with current value " +
      "previews), output memory keys to write, sibling-warning if " +
      "parallel, and the verbatim degraded-mode paragraph.",
    inputSchema: {
      type: "object",
      required: ["workflow_path", "phase_id"],
      properties: {
        workflow_path: { type: "string" },
        phase_id: { type: "string" },
        agent_index: {
          type: "integer",
          minimum: 0,
          description: "0-based index into the phase's agents list (default 0)",
        },
      },
    },
  },
];

const server = new Server(
  { name: "real-ruflo", version: "0.1.0" },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools }));

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  const { name, arguments: args } = req.params;
  try {
    switch (name) {
      case "memory.store":
        memory.store({
          namespace: args.namespace,
          key: args.key,
          value: args.value,
          tags: args.tags || null,
          embedding: args.embedding || null,
        });
        return content({ ok: true });

      case "memory.search":
        return content(
          memory.search({
            query: args.query,
            embedding: args.embedding || null,
            namespace: args.namespace || null,
            tags: args.tags || null,
            limit: args.limit || null,
            mode: args.mode || null,
          })
        );

      case "memory.delete":
        return content({ deleted: memory.delete(args.namespace, args.key) });

      case "memory.stats":
        return content(memory.stats());

      case "orchestrate.validate":
        return content(orchestrator.validate(args.workflow_path));

      case "orchestrate.status":
        return content(orchestrator.status(args.workflow_path));

      case "orchestrate.brief":
        return content(
          orchestrator.brief(
            args.workflow_path,
            args.phase_id,
            args.agent_index != null ? Number(args.agent_index) : null
          )
        );

      default:
        throw new Error(`Unknown tool: ${name}`);
    }
  } catch (err) {
    return {
      content: [{ type: "text", text: `error: ${err.message}` }],
      isError: true,
    };
  }
});

function content(value) {
  return {
    content: [{ type: "text", text: JSON.stringify(value, null, 2) }],
  };
}

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  console.error("real-ruflo: fatal:", err);
  process.exit(1);
});
