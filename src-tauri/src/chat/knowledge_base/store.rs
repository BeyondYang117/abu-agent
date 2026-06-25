//! SQLite-backed knowledge base store (sqlite-vec `vec0` + FTS5).
//!
//! Replaces the V1 JSON files. One SQLite file per library at
//! `{app_data}/knowledge_base/<kb_id>/store.db` (a `vec0` virtual table has a
//! fixed dimension, and each library binds its own embedding dim, so the vector
//! table is per-library and created lazily once the dim is known).
//!
//! Engineering red lines (see PRD V2 §D1): we use `rusqlite` directly (NOT
//! `tauri-plugin-sql`, whose sqlx can't load the vec extension); the extension
//! is registered via `sqlite3_auto_extension` before any connection opens;
//! `vec0` virtual tables can't carry extra columns, so chunk text/metadata live
//! in a normal `chunks` table joined by rowid.

use std::path::Path;
use std::sync::Once;

use rusqlite::{Connection, OpenFlags};

use super::{DocStatus, KnowledgeChunk, KnowledgeDocument};

/// Register sqlite-vec once per process so every opened connection sees `vec0`.
fn ensure_vec_extension() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // SAFETY: the documented sqlite-vec + rusqlite registration. Must run
        // before any connection is opened; `Once` guarantees single execution.
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

/// Serialize an embedding to the little-endian f32 blob sqlite-vec expects.
pub fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Open (creating if needed) a library's store.db with base tables. The `vec0`
/// table is created separately once the embedding dim is known.
pub fn open_db(path: &Path) -> Result<Connection, String> {
    ensure_vec_extension();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create kb dir: {e}"))?;
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )
    .map_err(|e| format!("open store.db: {e}"))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS documents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            hash TEXT NOT NULL DEFAULT '',
            chunk_count INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'indexing',
            error TEXT,
            created_at INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS chunks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            chunk_id TEXT NOT NULL,
            doc_id TEXT NOT NULL,
            doc_name TEXT NOT NULL DEFAULT '',
            text TEXT NOT NULL,
            heading_path TEXT,
            page INTEGER,
            char_start INTEGER NOT NULL DEFAULT 0,
            char_end INTEGER NOT NULL DEFAULT 0,
            order_index INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_chunks_doc ON chunks(doc_id);
         CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
            text, content='chunks', content_rowid='id'
         );
         CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
            INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
         END;
         CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES('delete', old.id, old.text);
         END;",
    )
    .map_err(|e| format!("init store.db schema: {e}"))?;
    Ok(conn)
}

/// Create the per-library `vec0` table for the given dimension if absent.
/// Cosine distance to match V1 retrieval semantics.
pub fn ensure_vec_table(conn: &Connection, dim: usize) -> Result<(), String> {
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(embedding float[{dim}] distance_metric=cosine);"
    ))
    .map_err(|e| format!("create vec_chunks(dim={dim}): {e}"))
}

// ===== documents =====

fn status_str(s: DocStatus) -> &'static str {
    match s {
        DocStatus::Indexing => "indexing",
        DocStatus::Ready => "ready",
        DocStatus::Error => "error",
    }
}

fn status_from(s: &str) -> DocStatus {
    match s {
        "ready" => DocStatus::Ready,
        "error" => DocStatus::Error,
        _ => DocStatus::Indexing,
    }
}

fn row_to_doc(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeDocument> {
    let status: String = row.get(5)?;
    Ok(KnowledgeDocument {
        id: row.get(0)?,
        name: row.get(1)?,
        size_bytes: row.get::<_, i64>(2)? as u64,
        hash: row.get(3)?,
        chunk_count: row.get::<_, i64>(4)? as usize,
        status: status_from(&status),
        error: row.get(6)?,
        created_at: row.get(7)?,
    })
}

const DOC_COLS: &str = "id, name, size_bytes, hash, chunk_count, status, error, created_at";

pub fn load_docs(conn: &Connection) -> Result<Vec<KnowledgeDocument>, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT {DOC_COLS} FROM documents ORDER BY created_at"))
        .map_err(|e| e.to_string())?;
    let docs = stmt
        .query_map([], row_to_doc)
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(docs)
}

pub fn doc_by_hash(conn: &Connection, hash: &str) -> Result<Option<KnowledgeDocument>, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT {DOC_COLS} FROM documents WHERE hash = ?1"))
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map([hash], row_to_doc).map_err(|e| e.to_string())?;
    match rows.next() {
        Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
        None => Ok(None),
    }
}

pub fn insert_doc(conn: &Connection, doc: &KnowledgeDocument) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO documents (id, name, size_bytes, hash, chunk_count, status, error, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            doc.id,
            doc.name,
            doc.size_bytes as i64,
            doc.hash,
            doc.chunk_count as i64,
            status_str(doc.status),
            doc.error,
            doc.created_at,
        ],
    )
    .map_err(|e| format!("insert doc: {e}"))?;
    Ok(())
}

pub fn set_doc_status(
    conn: &Connection,
    doc_id: &str,
    status: DocStatus,
    chunk_count: usize,
    error: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE documents SET status=?2, chunk_count=?3, error=?4 WHERE id=?1",
        rusqlite::params![doc_id, status_str(status), chunk_count as i64, error],
    )
    .map_err(|e| format!("set doc status: {e}"))?;
    Ok(())
}

/// Delete a document, its chunks (FTS auto-synced by trigger) and vec rows.
pub fn delete_doc(conn: &Connection, doc_id: &str) -> Result<bool, String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    delete_doc_vec_rows(&tx, doc_id)?;
    tx.execute("DELETE FROM chunks WHERE doc_id=?1", [doc_id])
        .map_err(|e| format!("delete chunks: {e}"))?;
    let n = tx
        .execute("DELETE FROM documents WHERE id=?1", [doc_id])
        .map_err(|e| format!("delete doc: {e}"))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(n > 0)
}

/// Remove vec rows whose rowid belongs to this doc's chunks (vtable has no FK).
fn delete_doc_vec_rows(conn: &Connection, doc_id: &str) -> Result<(), String> {
    if !vec_table_exists(conn)? {
        return Ok(());
    }
    conn.execute(
        "DELETE FROM vec_chunks WHERE rowid IN (SELECT id FROM chunks WHERE doc_id=?1)",
        [doc_id],
    )
    .map_err(|e| format!("delete vec rows: {e}"))?;
    Ok(())
}

fn vec_table_exists(conn: &Connection) -> Result<bool, String> {
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='vec_chunks'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

// ===== chunks =====

/// Replace all of a document's chunks (delete old + insert new) in one tx.
/// `dim` is the embedding dimension (used to lazily create the vec table).
pub fn replace_doc_chunks(
    conn: &Connection,
    doc_id: &str,
    dim: usize,
    chunks: &[KnowledgeChunk],
) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    if dim > 0 {
        ensure_vec_table(&tx, dim)?;
    }
    delete_doc_vec_rows(&tx, doc_id)?;
    tx.execute("DELETE FROM chunks WHERE doc_id=?1", [doc_id])
        .map_err(|e| format!("delete old chunks: {e}"))?;
    for c in chunks {
        tx.execute(
            "INSERT INTO chunks (chunk_id, doc_id, doc_name, text, heading_path, page, char_start, char_end, order_index)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            rusqlite::params![
                c.id,
                c.doc_id,
                c.doc_name,
                c.text,
                c.heading_path,
                c.page.map(|p| p as i64),
                c.char_start as i64,
                c.char_end as i64,
                c.order_index as i64,
            ],
        )
        .map_err(|e| format!("insert chunk: {e}"))?;
        let rowid = tx.last_insert_rowid();
        if dim > 0 {
            tx.execute(
                "INSERT INTO vec_chunks(rowid, embedding) VALUES (?1, ?2)",
                rusqlite::params![rowid, embedding_to_blob(&c.embedding)],
            )
            .map_err(|e| format!("insert vec: {e}"))?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// Drop every chunk + vec row (used by full reindex before refilling).
pub fn clear_chunks(conn: &Connection) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    if vec_table_exists(&tx)? {
        tx.execute("DROP TABLE vec_chunks", [])
            .map_err(|e| format!("drop vec table: {e}"))?;
    }
    tx.execute("DELETE FROM chunks", [])
        .map_err(|e| format!("clear chunks: {e}"))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn counts(conn: &Connection) -> Result<(usize, usize), String> {
    let docs: i64 = conn
        .query_row("SELECT count(*) FROM documents", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let chunks: i64 = conn
        .query_row("SELECT count(*) FROM chunks", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok((docs as usize, chunks as usize))
}

// ===== search =====

fn row_to_chunk(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeChunk> {
    Ok(KnowledgeChunk {
        id: row.get("chunk_id")?,
        doc_id: row.get("doc_id")?,
        doc_name: row.get("doc_name")?,
        text: row.get("text")?,
        heading_path: row.get("heading_path")?,
        page: row.get::<_, Option<i64>>("page")?.map(|p| p as usize),
        char_start: row.get::<_, i64>("char_start")? as usize,
        char_end: row.get::<_, i64>("char_end")? as usize,
        order_index: row.get::<_, i64>("order_index")? as usize,
        embedding: Vec::new(), // not needed in search results
    })
}

/// Vector KNN via sqlite-vec. Returns (chunk, cosine_score) best-first.
/// `score = 1 - cosine_distance`.
pub fn vector_search(
    conn: &Connection,
    query: &[f32],
    top_k: usize,
) -> Result<Vec<(KnowledgeChunk, f32)>, String> {
    if top_k == 0 || query.is_empty() || !vec_table_exists(conn)? {
        return Ok(Vec::new());
    }
    let sql = "SELECT c.chunk_id, c.doc_id, c.doc_name, c.text, c.heading_path, c.page,
                      c.char_start, c.char_end, c.order_index, v.distance AS distance
               FROM vec_chunks v
               JOIN chunks c ON c.id = v.rowid
               WHERE v.embedding MATCH ?1 AND k = ?2
               ORDER BY v.distance";
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let blob = embedding_to_blob(query);
    let rows = stmt
        .query_map(rusqlite::params![blob, top_k as i64], |row| {
            let chunk = row_to_chunk(row)?;
            let distance: f64 = row.get("distance")?;
            Ok((chunk, 1.0 - distance as f32))
        })
        .map_err(|e| format!("vector search: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    // De-risk: confirm bundled SQLite has FTS5 and sqlite-vec's vec0 registers
    // and answers a KNN query end-to-end.
    #[test]
    fn sqlite_vec_and_fts5_smoke() {
        ensure_vec_extension();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE chunks(id INTEGER PRIMARY KEY, text TEXT);
             CREATE VIRTUAL TABLE chunks_fts USING fts5(text, content='chunks', content_rowid='id');
             CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
                INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
             END;
             CREATE VIRTUAL TABLE vec_chunks USING vec0(embedding float[3]);",
        )
        .expect("FTS5 + vec0 must be available");

        // vector rows
        for (rowid, v) in [(1i64, [1.0f32, 0.0, 0.0]), (2, [0.0, 1.0, 0.0]), (3, [0.9, 0.1, 0.0])] {
            conn.execute(
                "INSERT INTO vec_chunks(rowid, embedding) VALUES (?, ?)",
                params![rowid, embedding_to_blob(&v)],
            )
            .unwrap();
        }
        // KNN: query near [1,0,0] → rowid 1 then 3
        let q = embedding_to_blob(&[1.0, 0.0, 0.0]);
        let rows: Vec<i64> = conn
            .prepare("SELECT rowid FROM vec_chunks WHERE embedding MATCH ? ORDER BY distance LIMIT 2")
            .unwrap()
            .query_map(params![q], |r| r.get::<_, i64>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(rows, vec![1, 3]);

        // FTS5 BM25
        conn.execute("INSERT INTO chunks(id, text) VALUES (1, 'the quick brown fox')", [])
            .unwrap();
        conn.execute("INSERT INTO chunks(id, text) VALUES (2, 'lazy dog sleeps')", [])
            .unwrap();
        let hits: Vec<i64> = conn
            .prepare("SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ? ORDER BY rank")
            .unwrap()
            .query_map(params!["fox"], |r| r.get::<_, i64>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(hits, vec![1]);
    }
}
