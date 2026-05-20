//! NAPI-rs bindings exposing `real-ruflo-memory` to Node.
//!
//! Thin wrappers. Every real decision lives in the Rust crate.

#![deny(clippy::all)]

use std::path::PathBuf;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use real_ruflo_memory::{
    default_db_path, MemoryStore, SearchMode, SearchRequest, StoreRequest,
};

#[napi(object)]
pub struct StoreArgs {
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    pub tags: Option<Vec<String>>,
    pub embedding: Option<Vec<f64>>,
}

#[napi(object)]
pub struct SearchArgs {
    pub query: String,
    pub embedding: Option<Vec<f64>>,
    pub namespace: Option<String>,
    pub tags: Option<Vec<String>>,
    pub limit: Option<u32>,
    /// "vector" | "lexical" | "hybrid" (default)
    pub mode: Option<String>,
}

#[napi]
pub struct Memory {
    inner: MemoryStore,
}

#[napi]
impl Memory {
    #[napi(constructor)]
    pub fn new(db_path: Option<String>) -> Result<Self> {
        let path: PathBuf = db_path.map(PathBuf::from).unwrap_or_else(default_db_path);
        let inner = MemoryStore::open(&path).map_err(to_napi)?;
        Ok(Self { inner })
    }

    #[napi]
    pub fn store(&self, args: StoreArgs) -> Result<()> {
        let req = StoreRequest {
            namespace: args.namespace,
            key: args.key,
            value: args.value,
            tags: args.tags.unwrap_or_default(),
            embedding: args.embedding.map(to_f32),
        };
        self.inner.store(&req).map_err(to_napi)
    }

    #[napi]
    pub fn get(&self, namespace: String, key: String) -> Result<serde_json::Value> {
        let entry = self.inner.get(&namespace, &key).map_err(to_napi)?;
        Ok(serde_json::to_value(entry).unwrap_or(serde_json::Value::Null))
    }

    #[napi]
    pub fn delete(&self, namespace: String, key: String) -> Result<bool> {
        self.inner.delete(&namespace, &key).map_err(to_napi)
    }

    #[napi]
    pub fn stats(&self) -> Result<serde_json::Value> {
        let stats = self.inner.stats().map_err(to_napi)?;
        Ok(serde_json::to_value(stats).unwrap_or(serde_json::Value::Null))
    }

    #[napi]
    pub fn search(&self, args: SearchArgs) -> Result<serde_json::Value> {
        let mode = match args.mode.as_deref() {
            Some("vector") => SearchMode::Vector,
            Some("lexical") => SearchMode::Lexical,
            _ => SearchMode::Hybrid,
        };
        let req = SearchRequest {
            query: args.query,
            embedding: args.embedding.map(to_f32),
            namespace: args.namespace,
            tags: args.tags.unwrap_or_default(),
            limit: args.limit.unwrap_or(10) as usize,
            mode,
        };
        let hits = self.inner.search(&req).map_err(to_napi)?;
        Ok(serde_json::to_value(hits).unwrap_or(serde_json::Value::Null))
    }
}

fn to_f32(v: Vec<f64>) -> Vec<f32> {
    v.into_iter().map(|x| x as f32).collect()
}

fn to_napi(e: real_ruflo_memory::MemoryError) -> Error {
    Error::new(Status::GenericFailure, e.to_string())
}
