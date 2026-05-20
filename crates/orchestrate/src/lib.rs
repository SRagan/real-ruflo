//! Declarative phase runner for lead-orchestrated multi-agent workflows.
//!
//! Real Ruflo cannot spawn Claude Code agents itself — the lead session
//! (you, talking to Claude Code) does that via the `Agent` tool. This
//! crate is the library + helpers the lead uses to:
//!
//! - **Declare** a multi-phase workflow in YAML
//! - **Validate** the dependency graph is a DAG with unique phase IDs
//! - **Check** which phases are ready, in progress, or done — derived from
//!   whether their output memory keys exist
//! - **Generate** a brief (markdown) for spawning each agent, including the
//!   degraded-mode paragraph and the current values of input memory keys
//!
//! It deliberately does NOT include a scheduler. There is no daemon spawning
//! agents; the lead agent reads the brief, calls the `Agent` tool, and
//! advances when memory shows the outputs are present.

use std::path::Path;

use thiserror::Error;

pub mod brief;
pub mod schema;
pub mod state;

pub use brief::generate_brief;
pub use schema::{AgentSpec, Output, Phase, Workflow};
pub use state::{phase_status, workflow_status, PhaseStatus, PhaseStatusKind, WorkflowStatus};

#[derive(Debug, Error)]
pub enum OrchestrateError {
    #[error("yaml parse error: {0}")]
    Yaml(#[from] serde_yml::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("memory error: {0}")]
    Memory(#[from] real_ruflo_memory::MemoryError),
    #[error("validation: {0}")]
    Validation(String),
    #[error("phase not found: {0}")]
    PhaseNotFound(String),
}

pub type Result<T> = std::result::Result<T, OrchestrateError>;

/// Load + validate a workflow from a YAML file on disk.
pub fn load_workflow_file(path: &Path) -> Result<Workflow> {
    let s = std::fs::read_to_string(path)?;
    load_workflow_str(&s)
}

/// Load + validate a workflow from a YAML string.
pub fn load_workflow_str(yaml: &str) -> Result<Workflow> {
    let wf: Workflow = serde_yml::from_str(yaml)?;
    wf.validate()?;
    Ok(wf)
}
