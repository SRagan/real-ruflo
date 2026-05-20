//! Pluggable embedding pipeline.
//!
//! The store does not require an embedder. Users may:
//!
//! 1. Supply pre-computed embeddings on `store(...)` from any source (OpenAI,
//!    Anthropic, Cohere, a local ONNX model, anything that produces floats).
//! 2. Plug in an [`Embedder`] implementation to compute them automatically.
//! 3. Skip embeddings entirely and rely on FTS5 lexical search.
//!
//! This trait is deliberately tiny so any model can satisfy it.

use std::sync::Arc;

/// A pluggable embedding source.
pub trait Embedder: Send + Sync {
    /// Dimensionality of the produced vector. Must be constant per instance.
    fn dim(&self) -> usize;

    /// Embed a single text into a fixed-dimensional vector.
    fn embed(&self, text: &str) -> Vec<f32>;
}

/// Default embedder: produces no embedding. Search falls back to lexical FTS5.
///
/// Use this when you don't want to ship an ML model. Real embedders (ONNX,
/// remote APIs) plug in as separate types implementing [`Embedder`].
#[derive(Default, Clone, Copy)]
pub struct NoEmbedder;

impl Embedder for NoEmbedder {
    fn dim(&self) -> usize {
        0
    }
    fn embed(&self, _text: &str) -> Vec<f32> {
        Vec::new()
    }
}

/// Wrapper that lets the store hold any embedder behind a thin Arc.
pub type DynEmbedder = Arc<dyn Embedder>;

/// Encode a vector to the BLOB representation stored in SQLite: little-endian
/// f32 values, tightly packed. Round-trips losslessly via [`decode_vector`].
pub fn encode_vector(v: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for &x in v {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    bytes
}

pub fn decode_vector(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    out
}

/// Cosine similarity between two equal-length vectors. Returns 0.0 if either
/// has zero magnitude.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = (na.sqrt()) * (nb.sqrt());
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_vector() {
        let v = vec![0.1, -0.2, 1.5, f32::MIN_POSITIVE];
        let bytes = encode_vector(&v);
        let back = decode_vector(&bytes);
        assert_eq!(v, back);
    }

    #[test]
    fn cosine_identity() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((cosine(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn cosine_empty_is_zero() {
        assert_eq!(cosine(&[], &[]), 0.0);
        assert_eq!(cosine(&[1.0], &[]), 0.0);
    }
}
