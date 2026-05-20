//! Markdown brief generation.
//!
//! Each spawned subagent gets a self-contained brief that includes:
//! - Role and which phase / workflow it's in
//! - What memory keys to read for context (with current values inlined)
//! - What memory keys to write
//! - The degraded-mode paragraph (verbatim — this is load-bearing)
//! - Success criteria

use std::fmt::Write;

use real_ruflo_memory::MemoryStore;

use crate::schema::AgentSpec;
use crate::{OrchestrateError, Result, Workflow};

/// Maximum length of an input value preview embedded in the brief. Keeps
/// briefs compact when memory values are large.
const PREVIEW_MAX_CHARS: usize = 600;

/// Build a markdown brief for a specific agent within a phase.
///
/// `agent_index` is the 0-based index into the phase's agents list. For
/// single-agent phases pass 0; for parallel groups, pass each index when
/// spawning each sibling.
pub fn generate_brief(
    workflow: &Workflow,
    phase_id: &str,
    agent_index: usize,
    memory: &MemoryStore,
) -> Result<String> {
    let phase = workflow.phase(phase_id)?;
    let agents = phase.agents.agents();
    let agent: &AgentSpec = agents.get(agent_index).ok_or_else(|| {
        OrchestrateError::Validation(format!(
            "agent index {} out of range for phase {} (has {} agents)",
            agent_index,
            phase_id,
            agents.len()
        ))
    })?;

    let mut out = String::with_capacity(2048);

    writeln!(out, "# Phase: `{}` → agent: `{}`", phase.id, agent.agent).unwrap();
    writeln!(out, "Workflow: **{}**", workflow.name).unwrap();
    writeln!(out, "Memory namespace: `{}`", workflow.namespace).unwrap();
    if let Some(desc) = &workflow.description {
        writeln!(out, "\n_{desc}_").unwrap();
    }

    if let Some(desc) = &phase.description {
        writeln!(out, "\n## Phase description\n{desc}").unwrap();
    }
    if let Some(desc) = &agent.description {
        writeln!(out, "\n## Your role\n{desc}").unwrap();
    }

    if agents.len() > 1 {
        let siblings: Vec<String> = agents
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != agent_index)
            .map(|(_, a)| a.agent.clone())
            .collect();
        writeln!(
            out,
            "\n## Parallel siblings\nYou are one of {} agents in this phase. \
             Your siblings: {}. They run in parallel; do NOT wait for them or \
             try to message them.",
            agents.len(),
            siblings.join(", ")
        )
        .unwrap();
    }

    if !agent.inputs.is_empty() {
        writeln!(out, "\n## Read these memory keys").unwrap();
        for key in &agent.inputs {
            let preview = preview_entry(memory, &workflow.namespace, key);
            writeln!(out, "\n### `{key}`\n{preview}").unwrap();
        }
    }

    if !agent.outputs.is_empty() {
        writeln!(out, "\n## Write to these memory keys (via `memory.store`)").unwrap();
        for out_decl in &agent.outputs {
            let hint = out_decl
                .hint()
                .map(|h| format!(" — {h}"))
                .unwrap_or_default();
            writeln!(out, "- `{}`{hint}", out_decl.key()).unwrap();
        }
    }

    writeln!(out, "\n## Degraded mode (read this)").unwrap();
    writeln!(out, "{}", degraded_mode_paragraph()).unwrap();

    writeln!(out, "\n## Success criteria").unwrap();
    writeln!(
        out,
        "Your phase is considered done when every output memory key listed \
         above exists in the `{}` namespace. The lead session re-checks memory \
         to decide when to advance — you do not need to signal completion.",
        workflow.namespace
    )
    .unwrap();

    Ok(out)
}

/// The verbatim degraded-mode paragraph. Honest about subagent constraints.
pub fn degraded_mode_paragraph() -> &'static str {
    "If your expected coordination tools (`SendMessage`, `TaskUpdate`, hive-mind, \
     etc.) are missing or fail, do NOT abort. Subagents in Claude Code cannot \
     reliably message each other (see ruvnet/ruflo#2028) — this is expected. \
     Read the source files or memory keys named above directly, write your \
     outputs to the memory keys named above using `memory.store`, and complete \
     your phase. Never block waiting for a message from a sibling agent — it \
     cannot reach you. The lead session will detect completion by reading \
     memory after you finish."
}

fn preview_entry(memory: &MemoryStore, namespace: &str, key: &str) -> String {
    match memory.get(namespace, key) {
        Ok(entry) => {
            let raw = match &entry.value {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            let clipped = if raw.chars().count() > PREVIEW_MAX_CHARS {
                let cut: String = raw.chars().take(PREVIEW_MAX_CHARS - 1).collect();
                format!("{cut}…")
            } else {
                raw
            };
            format!("```\n{clipped}\n```")
        }
        Err(_) => "_(not yet in memory — upstream phase has not produced this key)_".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_workflow_str;
    use real_ruflo_memory::{MemoryStore, StoreRequest};
    use tempfile::tempdir;

    const WF: &str = r#"
name: demo
namespace: demo-ns
description: A small workflow for testing
phases:
  - id: research
    description: Discover the lay of the land
    parallel:
      - agent: researcher
        description: Walk docs
        outputs:
          - key: research.inventory
            hint: JSON list of files
      - agent: code-reader
        outputs: [research.symbols]
  - id: design
    blockedBy: [research]
    agent: architect
    description: Synthesize findings
    inputs: [research.inventory, research.symbols]
    outputs: [design.proposal]
"#;

    fn store_for_namespace() -> (tempfile::TempDir, MemoryStore) {
        let dir = tempdir().unwrap();
        let s = MemoryStore::open(&dir.path().join("memory.db")).unwrap();
        (dir, s)
    }

    #[test]
    fn brief_for_parallel_agent_includes_sibling_warning() {
        let wf = load_workflow_str(WF).unwrap();
        let (_d, store) = store_for_namespace();
        let brief = generate_brief(&wf, "research", 0, &store).unwrap();
        assert!(brief.contains("Parallel siblings"));
        assert!(brief.contains("code-reader"));
        assert!(brief.contains("Degraded mode"));
        assert!(brief.contains("ruvnet/ruflo#2028"));
    }

    #[test]
    fn brief_for_consumer_phase_shows_input_preview() {
        let wf = load_workflow_str(WF).unwrap();
        let (_d, store) = store_for_namespace();
        store
            .store(&StoreRequest {
                namespace: wf.namespace.clone(),
                key: "research.inventory".into(),
                value: serde_json::json!(["file_a.rs", "file_b.rs"]),
                ..Default::default()
            })
            .unwrap();
        let brief = generate_brief(&wf, "design", 0, &store).unwrap();
        assert!(brief.contains("research.inventory"));
        assert!(brief.contains("file_a.rs"));
        assert!(brief.contains("research.symbols"));
        assert!(brief.contains("not yet in memory")); // for the absent input
    }

    #[test]
    fn brief_rejects_out_of_range_index() {
        let wf = load_workflow_str(WF).unwrap();
        let (_d, store) = store_for_namespace();
        let err = generate_brief(&wf, "research", 99, &store).unwrap_err();
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn brief_includes_output_hint() {
        let wf = load_workflow_str(WF).unwrap();
        let (_d, store) = store_for_namespace();
        let brief = generate_brief(&wf, "research", 0, &store).unwrap();
        assert!(brief.contains("research.inventory"));
        assert!(brief.contains("JSON list of files"));
    }
}
