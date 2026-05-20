//! Persistent semantic memory for Claude Code.
//!
//! Single SQLite-backed store. Vector + lexical + hybrid search.
//! BYO embeddings via the [`embed::Embedder`] trait.
//!
//! See `docs/design/memory.md` for the design rationale and
//! `ROADMAP.md` for scope and done-criteria.

use std::path::{Path, PathBuf};

use thiserror::Error;

pub mod embed;
pub mod schema;
pub mod search;
pub mod store;

pub use embed::{cosine, decode_vector, encode_vector, DynEmbedder, Embedder, NoEmbedder};
pub use search::{rrf, SearchMode, SearchRequest};
pub use store::{Entry, MemoryStats, MemoryStore, SearchHit, StoreRequest};

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found: namespace={namespace} key={key}")]
    NotFound { namespace: String, key: String },
}

pub type Result<T> = std::result::Result<T, MemoryError>;

pub fn default_db_path() -> PathBuf {
    dirs_path().join(".real-ruflo").join("memory.db")
}

fn dirs_path() -> PathBuf {
    if let Some(home) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(home);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home);
    }
    PathBuf::from(".")
}

pub fn ensure_parent(db_path: &Path) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
