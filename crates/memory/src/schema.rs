//! Schema definition + forward-only migrations.
//!
//! Versioned via `PRAGMA user_version`. Once a version ships, it is never
//! modified — a new version is appended that migrates from the previous.

use rusqlite::Connection;

use crate::Result;

pub const CURRENT_VERSION: i32 = 2;

pub fn migrate(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= CURRENT_VERSION {
        return Ok(());
    }
    if version < 1 {
        v1(conn)?;
    }
    if version < 2 {
        v2(conn)?;
    }
    conn.pragma_update(None, "user_version", CURRENT_VERSION)?;
    Ok(())
}

fn v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS entries (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            namespace    TEXT    NOT NULL,
            key          TEXT    NOT NULL,
            value        TEXT    NOT NULL,
            content_hash BLOB    NOT NULL,
            created_at   INTEGER NOT NULL,
            accessed_at  INTEGER NOT NULL,
            access_count INTEGER NOT NULL DEFAULT 0,
            UNIQUE(namespace, key)
        );

        CREATE INDEX IF NOT EXISTS idx_entries_ns_hash
            ON entries(namespace, content_hash);

        CREATE INDEX IF NOT EXISTS idx_entries_accessed
            ON entries(accessed_at);
        "#,
    )?;
    Ok(())
}

/// v2: add tags, embeddings, and FTS5 lexical search.
fn v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        ALTER TABLE entries ADD COLUMN tags      TEXT NOT NULL DEFAULT '[]';
        ALTER TABLE entries ADD COLUMN embedding BLOB;
        ALTER TABLE entries ADD COLUMN embed_dim INTEGER;

        CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
            value,
            tags,
            content='entries',
            content_rowid='id',
            tokenize='porter unicode61'
        );

        CREATE TRIGGER IF NOT EXISTS entries_fts_ai AFTER INSERT ON entries BEGIN
            INSERT INTO entries_fts(rowid, value, tags) VALUES (new.id, new.value, new.tags);
        END;
        CREATE TRIGGER IF NOT EXISTS entries_fts_ad AFTER DELETE ON entries BEGIN
            INSERT INTO entries_fts(entries_fts, rowid, value, tags)
                VALUES('delete', old.id, old.value, old.tags);
        END;
        CREATE TRIGGER IF NOT EXISTS entries_fts_au AFTER UPDATE ON entries BEGIN
            INSERT INTO entries_fts(entries_fts, rowid, value, tags)
                VALUES('delete', old.id, old.value, old.tags);
            INSERT INTO entries_fts(rowid, value, tags) VALUES (new.id, new.value, new.tags);
        END;
        "#,
    )?;
    Ok(())
}
