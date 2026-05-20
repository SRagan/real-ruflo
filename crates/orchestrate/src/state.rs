//! Derive phase status from memory contents.
//!
//! A phase is *done* when every output key declared by every agent in the
//! phase exists in the configured memory namespace. The system never tries
//! to track "in progress" itself — that would require a daemon. Instead, the
//! lead agent kicks off work, and `phase_status` re-derives the truth from
//! memory at any time.

use serde::{Deserialize, Serialize};

use real_ruflo_memory::MemoryStore;

use crate::{Phase, Result, Workflow};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatusKind {
    /// All declared output keys for the phase exist in memory.
    Done,
    /// Some — but not all — output keys exist. Phase is partially complete.
    Partial,
    /// Phase's `blockedBy` dependencies are all done, but the phase itself has
    /// no outputs yet.
    Ready,
    /// At least one `blockedBy` dependency is not yet done.
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseStatus {
    pub id: String,
    pub kind: PhaseStatusKind,
    pub outputs_total: usize,
    pub outputs_present: usize,
    pub missing_outputs: Vec<String>,
    pub blocked_by_open: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStatus {
    pub workflow: String,
    pub namespace: String,
    pub phases: Vec<PhaseStatus>,
    pub ready_phase_ids: Vec<String>,
    pub done_count: usize,
    pub total_count: usize,
}

pub fn workflow_status(workflow: &Workflow, memory: &MemoryStore) -> Result<WorkflowStatus> {
    let mut phase_statuses: Vec<PhaseStatus> = Vec::with_capacity(workflow.phases.len());
    for phase in &workflow.phases {
        phase_statuses.push(compute_phase_status(
            workflow,
            phase,
            memory,
            &phase_statuses,
        )?);
    }

    let done_count = phase_statuses
        .iter()
        .filter(|p| p.kind == PhaseStatusKind::Done)
        .count();

    let ready_phase_ids: Vec<String> = phase_statuses
        .iter()
        .filter(|p| matches!(p.kind, PhaseStatusKind::Ready | PhaseStatusKind::Partial))
        .map(|p| p.id.clone())
        .collect();

    Ok(WorkflowStatus {
        workflow: workflow.name.clone(),
        namespace: workflow.namespace.clone(),
        phases: phase_statuses,
        ready_phase_ids,
        done_count,
        total_count: workflow.phases.len(),
    })
}

pub fn phase_status(
    workflow: &Workflow,
    phase_id: &str,
    memory: &MemoryStore,
) -> Result<PhaseStatus> {
    let phase = workflow.phase(phase_id)?;
    // Compute predecessors first so we can decide Blocked vs Ready.
    let predecessor_statuses: Vec<PhaseStatus> = workflow
        .phases
        .iter()
        .filter(|p| phase.blocked_by.contains(&p.id))
        .map(|p| compute_phase_status(workflow, p, memory, &[]))
        .collect::<Result<_>>()?;
    compute_phase_status(workflow, phase, memory, &predecessor_statuses)
}

fn compute_phase_status(
    workflow: &Workflow,
    phase: &Phase,
    memory: &MemoryStore,
    already_computed: &[PhaseStatus],
) -> Result<PhaseStatus> {
    let mut outputs_total = 0usize;
    let mut outputs_present = 0usize;
    let mut missing: Vec<String> = Vec::new();

    for agent in phase.agents.agents() {
        for out in &agent.outputs {
            outputs_total += 1;
            if memory
                .get(&workflow.namespace, out.key())
                .map(|_| true)
                .unwrap_or(false)
            {
                outputs_present += 1;
            } else {
                missing.push(out.key().to_string());
            }
        }
    }

    let predecessors_done = phase.blocked_by.iter().all(|dep_id| {
        already_computed
            .iter()
            .find(|p| &p.id == dep_id)
            .map(|p| p.kind == PhaseStatusKind::Done)
            .unwrap_or(false)
    });
    let blocked_by_open: Vec<String> = phase
        .blocked_by
        .iter()
        .filter(|dep_id| {
            already_computed
                .iter()
                .find(|p| &&p.id == dep_id)
                .map(|p| p.kind != PhaseStatusKind::Done)
                .unwrap_or(true)
        })
        .cloned()
        .collect();

    let kind = if outputs_total > 0 && outputs_present == outputs_total {
        PhaseStatusKind::Done
    } else if outputs_present > 0 {
        PhaseStatusKind::Partial
    } else if predecessors_done {
        PhaseStatusKind::Ready
    } else {
        PhaseStatusKind::Blocked
    };

    Ok(PhaseStatus {
        id: phase.id.clone(),
        kind,
        outputs_total,
        outputs_present,
        missing_outputs: missing,
        blocked_by_open,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_workflow_str;
    use real_ruflo_memory::{MemoryStore, StoreRequest};
    use tempfile::tempdir;

    fn open_mem(ns: &str) -> (tempfile::TempDir, MemoryStore) {
        let dir = tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("memory.db")).unwrap();
        // Touch namespace so the workflow has somewhere to look (no-op).
        let _ = ns;
        (dir, store)
    }

    const WF: &str = r#"
name: t
namespace: t-ns
phases:
  - id: a
    agent: r
    outputs: [a.out]
  - id: b
    blockedBy: [a]
    agent: c
    outputs: [b.out]
"#;

    #[test]
    fn fresh_workflow_a_ready_b_blocked() {
        let wf = load_workflow_str(WF).unwrap();
        let (_d, store) = open_mem(&wf.namespace);
        let status = workflow_status(&wf, &store).unwrap();
        let a = &status.phases[0];
        let b = &status.phases[1];
        assert_eq!(a.kind, PhaseStatusKind::Ready);
        assert_eq!(b.kind, PhaseStatusKind::Blocked);
        assert_eq!(status.done_count, 0);
        assert_eq!(status.ready_phase_ids, vec!["a"]);
    }

    #[test]
    fn after_a_completes_b_becomes_ready() {
        let wf = load_workflow_str(WF).unwrap();
        let (_d, store) = open_mem(&wf.namespace);
        store
            .store(&StoreRequest {
                namespace: wf.namespace.clone(),
                key: "a.out".into(),
                value: serde_json::json!("done"),
                ..Default::default()
            })
            .unwrap();
        let status = workflow_status(&wf, &store).unwrap();
        assert_eq!(status.phases[0].kind, PhaseStatusKind::Done);
        assert_eq!(status.phases[1].kind, PhaseStatusKind::Ready);
        assert_eq!(status.done_count, 1);
    }

    #[test]
    fn full_completion_done_count_matches_total() {
        let wf = load_workflow_str(WF).unwrap();
        let (_d, store) = open_mem(&wf.namespace);
        for key in ["a.out", "b.out"] {
            store
                .store(&StoreRequest {
                    namespace: wf.namespace.clone(),
                    key: key.into(),
                    value: serde_json::json!("done"),
                    ..Default::default()
                })
                .unwrap();
        }
        let status = workflow_status(&wf, &store).unwrap();
        assert_eq!(status.done_count, status.total_count);
        assert!(status.ready_phase_ids.is_empty());
    }
}
