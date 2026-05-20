//! Core store API. Vector + lexical + hybrid search built in from v0.

use std::path::Path;

use chrono::Utc;
use rusqlite::{params, params_from_iter, Connection};
use serde::{Deserialize, Serialize};

use crate::embed::{cosine, decode_vector, encode_vector, DynEmbedder, NoEmbedder};
use crate::search::{rrf, SearchMode, SearchRequest};
use crate::{ensure_parent, schema, MemoryError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: i64,
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub accessed_at: i64,
    pub access_count: i64,
    pub embed_dim: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub entry: Entry,
    pub score: f32,
    pub source: &'static str, // "vector" | "lexical" | "hybrid"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_entries: i64,
    pub namespaces: i64,
    pub entries_with_embeddings: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoreRequest {
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Pre-computed embedding. If absent, the configured embedder runs.
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
}

pub struct MemoryStore {
    conn: Connection,
    embedder: DynEmbedder,
}

impl MemoryStore {
    /// Open with the default [`NoEmbedder`] (lexical-only). Use
    /// [`MemoryStore::open_with`] to plug in an embedder.
    pub fn open(db_path: &Path) -> Result<Self> {
        Self::open_with(db_path, std::sync::Arc::new(NoEmbedder))
    }

    pub fn open_with(db_path: &Path, embedder: DynEmbedder) -> Result<Self> {
        ensure_parent(db_path)?;
        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        schema::migrate(&conn)?;
        Ok(Self { conn, embedder })
    }

    pub fn store(&self, req: &StoreRequest) -> Result<()> {
        let serialized = serde_json::to_string(&req.value)?;
        let hash = blake3::hash(normalize(&serialized).as_bytes());
        let now = Utc::now().timestamp_millis();
        let tags_json = serde_json::to_string(&req.tags)?;

        let embedding: Option<Vec<f32>> = match &req.embedding {
            Some(e) => Some(e.clone()),
            None if self.embedder.dim() > 0 => Some(self.embedder.embed(&serialized)),
            _ => None,
        };
        let (embed_blob, embed_dim): (Option<Vec<u8>>, Option<i64>) = match &embedding {
            Some(v) if !v.is_empty() => (Some(encode_vector(v)), Some(v.len() as i64)),
            _ => (None, None),
        };

        self.conn.execute(
            r#"
            INSERT INTO entries
                (namespace, key, value, tags, content_hash, embedding, embed_dim,
                 created_at, accessed_at, access_count)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 0)
            ON CONFLICT(namespace, key) DO UPDATE SET
                value = excluded.value,
                tags = excluded.tags,
                content_hash = excluded.content_hash,
                embedding = excluded.embedding,
                embed_dim = excluded.embed_dim,
                accessed_at = excluded.accessed_at
            "#,
            params![
                &req.namespace,
                &req.key,
                serialized,
                tags_json,
                hash.as_bytes(),
                embed_blob,
                embed_dim,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, namespace: &str, key: &str) -> Result<Entry> {
        let entry = self
            .conn
            .query_row(
                r#"SELECT id, namespace, key, value, tags, created_at, accessed_at,
                          access_count, embed_dim
                   FROM entries WHERE namespace = ?1 AND key = ?2"#,
                params![namespace, key],
                row_to_entry,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => MemoryError::NotFound {
                    namespace: namespace.to_string(),
                    key: key.to_string(),
                },
                other => MemoryError::Sqlite(other),
            })?;

        let now = Utc::now().timestamp_millis();
        self.conn.execute(
            "UPDATE entries SET accessed_at = ?1, access_count = access_count + 1
             WHERE namespace = ?2 AND key = ?3",
            params![now, namespace, key],
        )?;
        Ok(entry)
    }

    pub fn delete(&self, namespace: &str, key: &str) -> Result<bool> {
        let rows = self.conn.execute(
            "DELETE FROM entries WHERE namespace = ?1 AND key = ?2",
            params![namespace, key],
        )?;
        Ok(rows > 0)
    }

    pub fn stats(&self) -> Result<MemoryStats> {
        let total: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))?;
        let namespaces: i64 = self
            .conn
            .query_row("SELECT COUNT(DISTINCT namespace) FROM entries", [], |r| {
                r.get(0)
            })?;
        let with_embeddings: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE embedding IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(MemoryStats {
            total_entries: total,
            namespaces,
            entries_with_embeddings: with_embeddings,
        })
    }

    pub fn search(&self, req: &SearchRequest) -> Result<Vec<SearchHit>> {
        let query_embedding: Option<Vec<f32>> = match (&req.embedding, req.mode) {
            (Some(e), _) => Some(e.clone()),
            (None, SearchMode::Lexical) => None,
            (None, _) if self.embedder.dim() > 0 => Some(self.embedder.embed(&req.query)),
            _ => None,
        };

        let vector_hits = if matches!(req.mode, SearchMode::Vector | SearchMode::Hybrid)
            && query_embedding.is_some()
        {
            self.vector_search(req, query_embedding.as_ref().unwrap())?
        } else {
            Vec::new()
        };

        let lexical_hits = if matches!(req.mode, SearchMode::Lexical | SearchMode::Hybrid) {
            self.lexical_search(req)?
        } else {
            Vec::new()
        };

        let combined = match req.mode {
            SearchMode::Vector => vector_hits,
            SearchMode::Lexical => lexical_hits,
            SearchMode::Hybrid => self.fuse(&vector_hits, &lexical_hits)?,
        };

        Ok(combined.into_iter().take(req.limit).collect())
    }

    fn vector_search(&self, req: &SearchRequest, query: &[f32]) -> Result<Vec<SearchHit>> {
        let (where_sql, bind) = filters(req);
        let sql = format!(
            "SELECT id, namespace, key, value, tags, created_at, accessed_at,
                    access_count, embed_dim, embedding
             FROM entries
             WHERE embedding IS NOT NULL {where_sql}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(bind.iter()), |row| {
            let blob: Vec<u8> = row.get("embedding")?;
            let entry = row_to_entry(row)?;
            Ok((entry, blob))
        })?;

        let mut hits: Vec<SearchHit> = Vec::new();
        for r in rows {
            let (entry, blob) = r?;
            let vec = decode_vector(&blob);
            if vec.len() != query.len() {
                continue;
            }
            let score = cosine(query, &vec);
            hits.push(SearchHit { entry, score, source: "vector" });
        }
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(req.limit * 4); // overfetch for fusion
        Ok(hits)
    }

    fn lexical_search(&self, req: &SearchRequest) -> Result<Vec<SearchHit>> {
        let (where_sql, mut bind) = filters(req);
        let fts_query = sanitize_fts(&req.query);
        let sql = format!(
            "SELECT e.id, e.namespace, e.key, e.value, e.tags, e.created_at, e.accessed_at,
                    e.access_count, e.embed_dim, bm25(entries_fts) AS bm
             FROM entries_fts
             JOIN entries e ON e.id = entries_fts.rowid
             WHERE entries_fts MATCH ?{n} {where_sql}
             ORDER BY bm
             LIMIT {lim}",
            n = bind.len() + 1,
            lim = req.limit * 4
        );
        bind.push(Box::new(fts_query));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(bind.iter()), |row| {
            let bm: f64 = row.get("bm")?;
            let entry = row_to_entry(row)?;
            // BM25 is lower-is-better; flip to similarity-ish in [0, 1].
            let score = 1.0 / (1.0 + (bm.max(0.0) as f32));
            Ok(SearchHit { entry, score, source: "lexical" })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn fuse(&self, vector: &[SearchHit], lexical: &[SearchHit]) -> Result<Vec<SearchHit>> {
        let vec_ids: Vec<i64> = vector.iter().map(|h| h.entry.id).collect();
        let lex_ids: Vec<i64> = lexical.iter().map(|h| h.entry.id).collect();
        let ranked = rrf(&[&vec_ids, &lex_ids], 60.0);

        // Rebuild full entries from whichever list contained them first.
        let by_id: std::collections::HashMap<i64, &SearchHit> = vector
            .iter()
            .chain(lexical.iter())
            .map(|h| (h.entry.id, h))
            .collect();

        let mut out = Vec::new();
        for (id, score) in ranked {
            if let Some(hit) = by_id.get(&id) {
                out.push(SearchHit {
                    entry: hit.entry.clone(),
                    score,
                    source: "hybrid",
                });
            }
        }
        Ok(out)
    }
}

type DynParam = Box<dyn rusqlite::ToSql>;

fn filters(req: &SearchRequest) -> (String, Vec<DynParam>) {
    let mut where_sql = String::new();
    let mut bind: Vec<DynParam> = Vec::new();

    if let Some(ns) = &req.namespace {
        where_sql.push_str(&format!(" AND namespace = ?{}", bind.len() + 1));
        bind.push(Box::new(ns.clone()));
    }
    // Tag filter: each tag must appear in the stored JSON tags array.
    // We use a substring match on the JSON, which is safe because tags
    // round-trip through serde_json (quoted strings).
    for tag in &req.tags {
        where_sql.push_str(&format!(" AND tags LIKE ?{}", bind.len() + 1));
        // Match `"tag"` to avoid prefix collisions.
        bind.push(Box::new(format!("%\"{}\"%", tag.replace('%', "\\%"))));
    }
    (where_sql, bind)
}

/// Strip FTS5 metacharacters from the user's query to avoid query-syntax errors.
fn sanitize_fts(q: &str) -> String {
    q.chars()
        .map(|c| match c {
            '"' | '*' | ':' | '(' | ')' => ' ',
            other => other,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    let value_str: String = row.get("value")?;
    let tags_str: String = row.get("tags")?;
    let value: serde_json::Value =
        serde_json::from_str(&value_str).unwrap_or(serde_json::Value::Null);
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
    Ok(Entry {
        id: row.get("id")?,
        namespace: row.get("namespace")?,
        key: row.get("key")?,
        value,
        tags,
        created_at: row.get("created_at")?,
        accessed_at: row.get("accessed_at")?,
        access_count: row.get("access_count")?,
        embed_dim: row.get("embed_dim")?,
    })
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::SearchMode;
    use tempfile::tempdir;

    fn open() -> (tempfile::TempDir, MemoryStore) {
        let dir = tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("memory.db")).unwrap();
        (dir, store)
    }

    fn req(ns: &str, key: &str, value: serde_json::Value) -> StoreRequest {
        StoreRequest {
            namespace: ns.to_string(),
            key: key.to_string(),
            value,
            tags: vec![],
            embedding: None,
        }
    }

    #[test]
    fn round_trip() {
        let (_d, store) = open();
        let value = serde_json::json!({ "hello": "world" });
        store.store(&req("test", "k1", value.clone())).unwrap();
        assert_eq!(store.get("test", "k1").unwrap().value, value);
        assert_eq!(store.stats().unwrap().total_entries, 1);
    }

    #[test]
    fn upsert_overwrites() {
        let (_d, store) = open();
        store.store(&req("ns", "k", serde_json::json!("first"))).unwrap();
        store.store(&req("ns", "k", serde_json::json!("second"))).unwrap();
        assert_eq!(store.get("ns", "k").unwrap().value, serde_json::json!("second"));
        assert_eq!(store.stats().unwrap().total_entries, 1);
    }

    #[test]
    fn delete_returns_true_when_present() {
        let (_d, store) = open();
        store.store(&req("ns", "k", serde_json::json!(1))).unwrap();
        assert!(store.delete("ns", "k").unwrap());
        assert!(!store.delete("ns", "k").unwrap());
    }

    #[test]
    fn lexical_search_finds_text() {
        let (_d, store) = open();
        store
            .store(&req("p", "a", serde_json::json!("we chose JWT for stateless auth")))
            .unwrap();
        store
            .store(&req("p", "b", serde_json::json!("postgres tuning notes")))
            .unwrap();

        let hits = store
            .search(&SearchRequest {
                query: "auth JWT".into(),
                embedding: None,
                namespace: None,
                tags: vec![],
                limit: 10,
                mode: SearchMode::Lexical,
            })
            .unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].entry.key, "a");
    }

    #[test]
    fn vector_search_uses_supplied_embedding() {
        let (_d, store) = open();
        let mut a = StoreRequest::default();
        a.namespace = "p".into();
        a.key = "a".into();
        a.value = serde_json::json!("apple");
        a.embedding = Some(vec![1.0, 0.0, 0.0]);
        store.store(&a).unwrap();

        let mut b = StoreRequest::default();
        b.namespace = "p".into();
        b.key = "b".into();
        b.value = serde_json::json!("banana");
        b.embedding = Some(vec![0.0, 1.0, 0.0]);
        store.store(&b).unwrap();

        let hits = store
            .search(&SearchRequest {
                query: String::new(),
                embedding: Some(vec![0.9, 0.1, 0.0]),
                namespace: None,
                tags: vec![],
                limit: 5,
                mode: SearchMode::Vector,
            })
            .unwrap();
        assert_eq!(hits[0].entry.key, "a");
    }

    #[test]
    fn tag_filter_excludes_unmatched() {
        let (_d, store) = open();
        let mut a = StoreRequest::default();
        a.namespace = "p".into();
        a.key = "a".into();
        a.value = serde_json::json!("decision about auth");
        a.tags = vec!["architecture".into(), "security".into()];
        store.store(&a).unwrap();

        let mut b = StoreRequest::default();
        b.namespace = "p".into();
        b.key = "b".into();
        b.value = serde_json::json!("decision about auth");
        b.tags = vec!["architecture".into()];
        store.store(&b).unwrap();

        let hits = store
            .search(&SearchRequest {
                query: "auth".into(),
                embedding: None,
                namespace: None,
                tags: vec!["security".into()],
                limit: 10,
                mode: SearchMode::Lexical,
            })
            .unwrap();
        let keys: Vec<&str> = hits.iter().map(|h| h.entry.key.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(!keys.contains(&"b"));
    }
}
