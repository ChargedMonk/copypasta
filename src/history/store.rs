use crate::clipboard::item::{ClipboardFormatPayload, ClipboardItem, PayloadStorage};
use anyhow::Context;
use directories::ProjectDirs;
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Store {
    conn: Connection,
    base_dir: PathBuf,
    max_total_bytes: u64,
}

impl Store {
    pub fn open() -> anyhow::Result<Self> {
        let proj = ProjectDirs::from("com", "VatsalyaBajpai", "Copypasta")
            .context("ProjectDirs not available")?;
        let base_dir = proj.data_local_dir().to_path_buf();
        fs::create_dir_all(&base_dir).context("create base data dir")?;

        let db_path = base_dir.join("copypasta.db");
        let conn = Connection::open(db_path).context("open sqlite db")?;

        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS items(
              id INTEGER PRIMARY KEY,
              created_unix_ms INTEGER NOT NULL,
              fingerprint BLOB NOT NULL UNIQUE,
              preview TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS payloads(
              item_id INTEGER NOT NULL,
              format INTEGER NOT NULL,
              rel_path TEXT NOT NULL,
              size INTEGER NOT NULL,
              PRIMARY KEY(item_id, format),
              FOREIGN KEY(item_id) REFERENCES items(id) ON DELETE CASCADE
            );
            "#,
        )?;

        Ok(Self {
            conn,
            base_dir,
            max_total_bytes: 250 * 1024 * 1024, // 250MB default
        })
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn load_recent(&self, limit: usize) -> anyhow::Result<Vec<ClipboardItem>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, created_unix_ms, fingerprint, preview
            FROM items
            ORDER BY created_unix_ms DESC
            LIMIT ?1
            "#,
        )?;
        let mut rows = stmt.query(params![limit as i64])?;

        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let created_unix_ms: i64 = row.get(1)?;
            let fp: Vec<u8> = row.get(2)?;
            let preview: String = row.get(3)?;
            let fingerprint: [u8; 32] = fp.as_slice().try_into().unwrap_or([0u8; 32]);

            let formats = self.load_payload_descriptors(id as u64)?;

            items.push(ClipboardItem {
                id: id as u64,
                created_unix_ms,
                fingerprint,
                preview,
                formats,
            });
        }
        Ok(items)
    }

    fn load_payload_descriptors(
        &self,
        item_id: u64,
    ) -> anyhow::Result<Vec<ClipboardFormatPayload>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT format, rel_path, size
            FROM payloads
            WHERE item_id = ?1
            ORDER BY format ASC
            "#,
        )?;
        let mut rows = stmt.query(params![item_id as i64])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let format: i64 = row.get(0)?;
            let rel_path: String = row.get(1)?;
            let size: i64 = row.get(2)?;
            out.push(ClipboardFormatPayload {
                format: format as u32,
                storage: PayloadStorage::File {
                    rel_path,
                    size: size as u64,
                },
            });
        }
        Ok(out)
    }

    pub fn persist_captured(
        &mut self,
        created_unix_ms: i64,
        fingerprint: [u8; 32],
        preview: &str,
        formats: &[ClipboardFormatPayload],
    ) -> anyhow::Result<ClipboardItem> {
        let item_id = self.upsert_item(created_unix_ms, &fingerprint, preview)?;

        let item_dir = self.base_dir.join("items").join(item_id.to_string());
        fs::create_dir_all(&item_dir).context("create item dir")?;

        for p in formats {
            let bytes = match &p.storage {
                PayloadStorage::Inline(b) => b.as_slice(),
                PayloadStorage::File { .. } => continue,
            };
            let filename = format!("fmt_{}.bin", p.format);
            let rel_path = format!("items/{}/{}", item_id, filename);
            let abs_path = self.base_dir.join(&rel_path);
            fs::write(&abs_path, bytes).with_context(|| format!("write payload {}", rel_path))?;

            self.conn.execute(
                r#"
                INSERT INTO payloads(item_id, format, rel_path, size)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(item_id, format) DO UPDATE SET
                  rel_path=excluded.rel_path,
                  size=excluded.size
                "#,
                params![
                    item_id as i64,
                    p.format as i64,
                    rel_path,
                    bytes.len() as i64
                ],
            )?;
        }

        self.prune_to_limits().ok();

        Ok(ClipboardItem {
            id: item_id,
            created_unix_ms,
            fingerprint,
            preview: preview.to_string(),
            formats: self.load_payload_descriptors(item_id)?,
        })
    }

    fn upsert_item(
        &mut self,
        created_unix_ms: i64,
        fingerprint: &[u8; 32],
        preview: &str,
    ) -> anyhow::Result<u64> {
        let fp: &[u8] = fingerprint;
        match self.conn.execute(
            r#"
            INSERT INTO items(created_unix_ms, fingerprint, preview)
            VALUES (?1, ?2, ?3)
            "#,
            params![created_unix_ms, fp, preview],
        ) {
            Ok(_) => Ok(self.conn.last_insert_rowid() as u64),
            Err(_) => {
                // Duplicate fingerprint: bump timestamp + preview.
                self.conn.execute(
                    r#"
                    UPDATE items SET created_unix_ms=?1, preview=?2 WHERE fingerprint=?3
                    "#,
                    params![created_unix_ms, preview, fp],
                )?;
                let id: i64 = self.conn.query_row(
                    "SELECT id FROM items WHERE fingerprint=?1",
                    params![fp],
                    |r| r.get(0),
                )?;
                Ok(id as u64)
            }
        }
    }

    fn prune_to_limits(&mut self) -> anyhow::Result<()> {
        // Rough disk usage via payload sizes recorded in DB.
        let total: i64 =
            self.conn
                .query_row("SELECT COALESCE(SUM(size), 0) FROM payloads", [], |r| {
                    r.get(0)
                })?;
        if total as u64 <= self.max_total_bytes {
            return Ok(());
        }

        let mut stmt = self.conn.prepare(
            r#"
            SELECT id FROM items ORDER BY created_unix_ms ASC
            "#,
        )?;
        let ids = stmt
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut current = total as u64;
        for id in ids {
            if current <= self.max_total_bytes {
                break;
            }
            let size: i64 = self.conn.query_row(
                "SELECT COALESCE(SUM(size), 0) FROM payloads WHERE item_id=?1",
                params![id],
                |r| r.get(0),
            )?;
            self.conn
                .execute("DELETE FROM items WHERE id=?1", params![id])?;

            let item_dir = self.base_dir.join("items").join(id.to_string());
            let _ = fs::remove_dir_all(item_dir);

            current = current.saturating_sub(size as u64);
        }

        Ok(())
    }

    pub fn clear_all(&mut self) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM items", [])?;
        let items_dir = self.base_dir.join("items");
        let _ = fs::remove_dir_all(&items_dir);
        let _ = fs::create_dir_all(&items_dir);
        Ok(())
    }
}
