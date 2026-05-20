//! Workflow YAML schema + DAG validation.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{OrchestrateError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub namespace: String,
    #[serde(default)]
    pub description: Option<String>,
    pub phases: Vec<Phase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "blockedBy")]
    pub blocked_by: Vec<String>,
    #[serde(flatten)]
    pub agents: AgentBlock,
}

/// A phase contains either a single agent or a parallel group.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentBlock {
    Parallel { parallel: Vec<AgentSpec> },
    Single(AgentSpec),
}

impl AgentBlock {
    pub fn agents(&self) -> &[AgentSpec] {
        match self {
            AgentBlock::Parallel { parallel } => parallel,
            AgentBlock::Single(spec) => std::slice::from_ref(spec),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    /// Logical role / type, e.g. "researcher", "coder", "architect".
    /// The lead agent maps this to a Claude Code subagent type.
    pub agent: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<Output>,
}

/// An output declaration. Accepts either a bare string (the memory key) or an
/// object with key + hint to help the spawned agent understand what should
/// land there.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Output {
    Key(String),
    Detailed { key: String, hint: Option<String> },
}

impl Output {
    pub fn key(&self) -> &str {
        match self {
            Output::Key(k) => k,
            Output::Detailed { key, .. } => key,
        }
    }
    pub fn hint(&self) -> Option<&str> {
        match self {
            Output::Key(_) => None,
            Output::Detailed { hint, .. } => hint.as_deref(),
        }
    }
}

impl Workflow {
    /// Validate: unique phase IDs, blockedBy references exist, no cycles,
    /// no phase blocks on itself, at least one phase, no duplicate output
    /// keys across the workflow.
    pub fn validate(&self) -> Result<()> {
        if self.phases.is_empty() {
            return Err(OrchestrateError::Validation(
                "workflow has no phases".into(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(OrchestrateError::Validation(
                "workflow name is empty".into(),
            ));
        }
        if self.namespace.trim().is_empty() {
            return Err(OrchestrateError::Validation(
                "workflow namespace is empty".into(),
            ));
        }

        // Unique phase IDs.
        let mut ids: HashSet<&str> = HashSet::new();
        for p in &self.phases {
            if !ids.insert(&p.id) {
                return Err(OrchestrateError::Validation(format!(
                    "duplicate phase id: {}",
                    p.id
                )));
            }
        }

        // blockedBy references must exist; no self-loop.
        for p in &self.phases {
            for dep in &p.blocked_by {
                if dep == &p.id {
                    return Err(OrchestrateError::Validation(format!(
                        "phase {} blocks on itself",
                        p.id
                    )));
                }
                if !ids.contains(dep.as_str()) {
                    return Err(OrchestrateError::Validation(format!(
                        "phase {} blockedBy unknown phase {}",
                        p.id, dep
                    )));
                }
            }
        }

        // Unique output keys across the whole workflow.
        let mut seen_outputs: HashSet<String> = HashSet::new();
        for p in &self.phases {
            for agent in p.agents.agents() {
                for out in &agent.outputs {
                    if !seen_outputs.insert(out.key().to_string()) {
                        return Err(OrchestrateError::Validation(format!(
                            "output key declared twice: {}",
                            out.key()
                        )));
                    }
                }
            }
        }

        // DAG check via topological sort (Kahn).
        self.topo_order()?;

        Ok(())
    }

    /// Return phases in a valid execution order. Errors if there's a cycle.
    pub fn topo_order(&self) -> Result<Vec<&Phase>> {
        let mut indegree: HashMap<&str, usize> = HashMap::new();
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for p in &self.phases {
            indegree.entry(p.id.as_str()).or_insert(0);
            for dep in &p.blocked_by {
                *indegree.entry(p.id.as_str()).or_insert(0) += 1;
                adjacency.entry(dep.as_str()).or_default().push(&p.id);
            }
        }

        let mut queue: Vec<&str> = indegree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| *id)
            .collect();
        queue.sort();

        let mut ordered_ids: Vec<&str> = Vec::new();
        while let Some(id) = queue.pop() {
            ordered_ids.push(id);
            if let Some(next_ids) = adjacency.get(id) {
                for n in next_ids {
                    let entry = indegree.get_mut(*n).unwrap();
                    *entry -= 1;
                    if *entry == 0 {
                        queue.push(n);
                    }
                }
            }
        }

        if ordered_ids.len() != self.phases.len() {
            return Err(OrchestrateError::Validation(
                "workflow has a cycle in blockedBy dependencies".into(),
            ));
        }

        let by_id: HashMap<&str, &Phase> = self.phases.iter().map(|p| (p.id.as_str(), p)).collect();
        Ok(ordered_ids.into_iter().map(|id| by_id[id]).collect())
    }

    pub fn phase(&self, id: &str) -> Result<&Phase> {
        self.phases
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| OrchestrateError::PhaseNotFound(id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use crate::load_workflow_str;

    const VALID: &str = r#"
name: codebase-audit
namespace: real-ruflo-test
phases:
  - id: research
    parallel:
      - agent: researcher
        outputs: [research.inventory]
      - agent: code-reader
        outputs: [research.symbols]
  - id: design
    blockedBy: [research]
    agent: architect
    inputs: [research.inventory, research.symbols]
    outputs: [design.proposal]
"#;

    #[test]
    fn parses_valid_workflow() {
        let wf = load_workflow_str(VALID).unwrap();
        assert_eq!(wf.name, "codebase-audit");
        assert_eq!(wf.phases.len(), 2);
        let order = wf.topo_order().unwrap();
        // research must come before design
        let pos: std::collections::HashMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.as_str(), i))
            .collect();
        assert!(pos["research"] < pos["design"]);
    }

    #[test]
    fn rejects_duplicate_phase_id() {
        let yaml = r#"
name: x
namespace: ns
phases:
  - id: a
    agent: x
    outputs: [k1]
  - id: a
    agent: y
    outputs: [k2]
"#;
        let err = load_workflow_str(yaml).unwrap_err().to_string();
        assert!(err.contains("duplicate phase id"));
    }

    #[test]
    fn rejects_unknown_dependency() {
        let yaml = r#"
name: x
namespace: ns
phases:
  - id: a
    blockedBy: [ghost]
    agent: x
    outputs: [k]
"#;
        let err = load_workflow_str(yaml).unwrap_err().to_string();
        assert!(err.contains("unknown phase ghost"));
    }

    #[test]
    fn rejects_cycle() {
        let yaml = r#"
name: x
namespace: ns
phases:
  - id: a
    blockedBy: [b]
    agent: x
    outputs: [ak]
  - id: b
    blockedBy: [a]
    agent: y
    outputs: [bk]
"#;
        let err = load_workflow_str(yaml).unwrap_err().to_string();
        assert!(err.contains("cycle"));
    }

    #[test]
    fn rejects_duplicate_output_key() {
        let yaml = r#"
name: x
namespace: ns
phases:
  - id: a
    agent: x
    outputs: [same]
  - id: b
    agent: y
    outputs: [same]
"#;
        let err = load_workflow_str(yaml).unwrap_err().to_string();
        assert!(err.contains("declared twice"));
    }

    #[test]
    fn detailed_output_hint_round_trips() {
        let yaml = r#"
name: x
namespace: ns
phases:
  - id: a
    agent: r
    outputs:
      - key: k1
        hint: a sentence about k1
"#;
        let wf = load_workflow_str(yaml).unwrap();
        let out = &wf.phases[0].agents.agents()[0].outputs[0];
        assert_eq!(out.key(), "k1");
        assert_eq!(out.hint(), Some("a sentence about k1"));
    }
}
