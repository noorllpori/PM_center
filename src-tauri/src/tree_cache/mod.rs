use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct FsEntrySnapshot {
    pub path: String,
    pub parent_path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ts: Option<i64>,
    pub created_ts: Option<i64>,
    pub extension: Option<String>,
    pub last_seen_ts: i64,
}

#[derive(Clone)]
pub struct TreeCacheDb {
    conn: Arc<Mutex<Connection>>,
    project_path: String,
}

lazy_static::lazy_static! {
    static ref TREE_CACHE_DBS: Arc<Mutex<HashMap<String, TreeCacheDb>>> = Arc::new(Mutex::new(HashMap::new()));
}

pub fn normalize_path_key(path: &str) -> String {
    #[cfg(windows)]
    {
        path.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

pub fn detect_project_root_for_path(path: &str) -> Option<String> {
    let mut cursor = PathBuf::from(path);
    if cursor.is_file() {
        let _ = cursor.pop();
    }

    loop {
        let marker = cursor.join(".pm_center");
        if marker.exists() && marker.is_dir() {
            return Some(cursor.to_string_lossy().to_string());
        }

        if !cursor.pop() {
            break;
        }
    }

    None
}

pub fn get_or_create_project_cache(project_path: &str) -> Result<TreeCacheDb, String> {
    let project_key = normalize_path_key(project_path);
    {
        let guard = TREE_CACHE_DBS.lock().map_err(|error| error.to_string())?;
        if let Some(db) = guard.get(&project_key) {
            return Ok(db.clone());
        }
    }

    let db = TreeCacheDb::new(project_path)?;
    let mut guard = TREE_CACHE_DBS.lock().map_err(|error| error.to_string())?;
    let existing = guard
        .entry(project_key)
        .or_insert_with(|| db.clone())
        .clone();
    Ok(existing)
}

pub fn process_dirty_dirs(max_dirs_per_project: usize) -> Result<(), String> {
    let caches = {
        let guard = TREE_CACHE_DBS.lock().map_err(|error| error.to_string())?;
        guard.values().cloned().collect::<Vec<_>>()
    };

    for cache in caches {
        let dirty_dirs = cache.get_dirty_dirs(max_dirs_per_project)?;
        if dirty_dirs.is_empty() {
            continue;
        }

        for dir_path in dirty_dirs {
            let path = PathBuf::from(&dir_path);
            if !path.exists() || !path.is_dir() {
                cache.remove_path_subtree(&dir_path)?;
                cache.clear_dir_dirty(&dir_path)?;
                continue;
            }

            let entries = scan_directory_entries_from_disk(&cache.project_path, &dir_path)?;
            cache.replace_directory_entries(&dir_path, &entries)?;
        }

        if cache.count_dirty_dirs()? == 0 {
            cache.set_tree_clean()?;
        }
    }

    Ok(())
}

pub fn rebuild_project_tree_cache(project_path: &str) -> Result<(), String> {
    let cache = get_or_create_project_cache(project_path)?;
    let project_root = PathBuf::from(project_path);
    if !project_root.exists() || !project_root.is_dir() {
        return Err("project path is not a valid directory".to_string());
    }

    let now = now_ts();
    let mut entries = Vec::new();
    let mut scanned_dirs = Vec::new();
    let mut stack = vec![project_root.clone()];

    while let Some(current_dir) = stack.pop() {
        if should_skip_path(&project_root, &current_dir) {
            continue;
        }

        let current_dir_str = current_dir.to_string_lossy().to_string();
        scanned_dirs.push(current_dir_str.clone());

        let dir_entries = fs::read_dir(&current_dir)
            .map_err(|error| format!("failed to scan {}: {}", current_dir.display(), error))?;

        for entry_result in dir_entries {
            let entry = match entry_result {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let entry_path = entry.path();
            if should_skip_path(&project_root, &entry_path) {
                continue;
            }

            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };

            let entry_path_str = entry_path.to_string_lossy().to_string();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = metadata.is_dir();
            let extension = if is_dir {
                None
            } else {
                entry_path
                    .extension()
                    .map(|ext| ext.to_string_lossy().to_string().to_lowercase())
            };

            entries.push(FsEntrySnapshot {
                path: entry_path_str.clone(),
                parent_path: current_dir_str.clone(),
                name,
                is_dir,
                size: metadata.len(),
                modified_ts: metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs() as i64),
                created_ts: metadata
                    .created()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs() as i64),
                extension,
                last_seen_ts: now,
            });

            if is_dir {
                stack.push(entry_path);
            }
        }
    }

    cache.replace_all_entries(&entries, &scanned_dirs, now)?;
    Ok(())
}

pub fn scan_directory_entries_from_disk(
    project_path: &str,
    directory_path: &str,
) -> Result<Vec<FsEntrySnapshot>, String> {
    let project_root = PathBuf::from(project_path);
    let dir = PathBuf::from(directory_path);
    if !dir.exists() || !dir.is_dir() {
        return Ok(Vec::new());
    }

    let now = now_ts();
    let mut entries = Vec::new();
    let mut iterator = fs::read_dir(&dir)
        .map_err(|error| format!("failed to read {}: {}", directory_path, error))?;

    while let Some(entry_result) = iterator.next() {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let entry_path = entry.path();
        if should_skip_path(&project_root, &entry_path) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };

        let is_dir = metadata.is_dir();
        let extension = if is_dir {
            None
        } else {
            entry_path
                .extension()
                .map(|ext| ext.to_string_lossy().to_string().to_lowercase())
        };

        entries.push(FsEntrySnapshot {
            path: entry_path.to_string_lossy().to_string(),
            parent_path: directory_path.to_string(),
            name: entry.file_name().to_string_lossy().to_string(),
            is_dir,
            size: metadata.len(),
            modified_ts: metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64),
            created_ts: metadata
                .created()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64),
            extension,
            last_seen_ts: now,
        });
    }

    entries.sort_by(|left, right| match (left.is_dir, right.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
    });

    Ok(entries)
}

impl TreeCacheDb {
    fn new(project_path: &str) -> Result<Self, String> {
        let data_dir = PathBuf::from(project_path).join(".pm_center");
        fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;

        let db_path = data_dir.join("tree_cache.db");
        let schema_sql = r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA temp_store=MEMORY;

            CREATE TABLE IF NOT EXISTS fs_entries (
                path_key TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                parent_path TEXT NOT NULL,
                parent_path_key TEXT NOT NULL,
                name TEXT NOT NULL,
                is_dir INTEGER NOT NULL,
                size INTEGER NOT NULL,
                modified_ts INTEGER,
                created_ts INTEGER,
                extension TEXT,
                last_seen_ts INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS dir_state (
                dir_path_key TEXT PRIMARY KEY,
                dir_path TEXT NOT NULL,
                is_dirty INTEGER NOT NULL DEFAULT 0,
                last_scan_ts INTEGER,
                last_event_ts INTEGER
            );

            CREATE TABLE IF NOT EXISTS tree_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                is_dirty INTEGER NOT NULL DEFAULT 1,
                last_full_scan_ts INTEGER
            );

            CREATE TABLE IF NOT EXISTS file_details_cache (
                path_key TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                parent_path TEXT NOT NULL,
                parent_path_key TEXT NOT NULL,
                name TEXT NOT NULL,
                signature TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                last_access_ts INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_fs_entries_parent
            ON fs_entries(parent_path_key, is_dir, name);

            CREATE INDEX IF NOT EXISTS idx_file_details_parent_access
            ON file_details_cache(parent_path_key, last_access_ts);

            CREATE INDEX IF NOT EXISTS idx_file_details_expires_at
            ON file_details_cache(expires_at);
            "#;

        let mut conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
        if let Err(error) = conn.execute_batch(schema_sql) {
            let _ = fs::remove_file(&db_path);
            conn = Connection::open(&db_path).map_err(|open_error| open_error.to_string())?;
            conn.execute_batch(schema_sql).map_err(|schema_error| {
                format!("tree cache schema init failed: {}", schema_error)
            })?;
            eprintln!(
                "[TreeCache] schema init failed, recreated cache db: {}",
                error
            );
        }

        conn.execute(
            "INSERT OR IGNORE INTO tree_state (id, is_dirty, last_full_scan_ts) VALUES (1, 1, NULL)",
            [],
        )
        .map_err(|error| error.to_string())?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            project_path: project_path.to_string(),
        })
    }

    pub fn has_directory_snapshot(&self, dir_path: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let exists: Option<i64> = conn
            .query_row(
                "SELECT last_scan_ts FROM dir_state WHERE dir_path_key = ?1 LIMIT 1",
                params![normalize_path_key(dir_path)],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        Ok(exists.is_some())
    }

    pub fn get_directory_entries(&self, dir_path: &str) -> Result<Vec<FsEntrySnapshot>, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let mut statement = conn
            .prepare(
                r#"
                SELECT path, parent_path, name, is_dir, size, modified_ts, created_ts, extension, last_seen_ts
                FROM fs_entries
                WHERE parent_path_key = ?1
                ORDER BY is_dir DESC, name COLLATE NOCASE ASC
                "#,
            )
            .map_err(|error| error.to_string())?;

        let rows = statement
            .query_map(params![normalize_path_key(dir_path)], |row| {
                Ok(FsEntrySnapshot {
                    path: row.get(0)?,
                    parent_path: row.get(1)?,
                    name: row.get(2)?,
                    is_dir: row.get::<_, i64>(3)? == 1,
                    size: row.get::<_, i64>(4)? as u64,
                    modified_ts: row.get(5)?,
                    created_ts: row.get(6)?,
                    extension: row.get(7)?,
                    last_seen_ts: row.get(8)?,
                })
            })
            .map_err(|error| error.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn get_cached_child_dirs(&self, parent_path: &str) -> Result<Vec<String>, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let mut statement = conn
            .prepare(
                r#"
                SELECT path
                FROM fs_entries
                WHERE parent_path_key = ?1 AND is_dir = 1
                ORDER BY name COLLATE NOCASE ASC
                "#,
            )
            .map_err(|error| error.to_string())?;

        let rows = statement
            .query_map(params![normalize_path_key(parent_path)], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| error.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn replace_directory_entries(
        &self,
        dir_path: &str,
        entries: &[FsEntrySnapshot],
    ) -> Result<(), String> {
        let dir_key = normalize_path_key(dir_path);
        let now = now_ts();

        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;

        let previous_child_dirs: Vec<(String, String)> = {
            let mut statement = tx
                .prepare(
                    r#"
                    SELECT path, path_key
                    FROM fs_entries
                    WHERE parent_path_key = ?1 AND is_dir = 1
                    "#,
                )
                .map_err(|error| error.to_string())?;

            let rows = statement
                .query_map(params![dir_key.clone()], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .map_err(|error| error.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?
        };

        tx.execute(
            "DELETE FROM fs_entries WHERE parent_path_key = ?1",
            params![dir_key.clone()],
        )
        .map_err(|error| error.to_string())?;

        let mut inserted_dir_keys = HashSet::new();
        for entry in entries {
            let entry_path_key = normalize_path_key(&entry.path);
            let parent_path_key = normalize_path_key(&entry.parent_path);
            if entry.is_dir {
                inserted_dir_keys.insert(entry_path_key.clone());
            }

            tx.execute(
                r#"
                INSERT INTO fs_entries (
                    path_key, path, parent_path, parent_path_key, name,
                    is_dir, size, modified_ts, created_ts, extension, last_seen_ts
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(path_key) DO UPDATE SET
                    path = excluded.path,
                    parent_path = excluded.parent_path,
                    parent_path_key = excluded.parent_path_key,
                    name = excluded.name,
                    is_dir = excluded.is_dir,
                    size = excluded.size,
                    modified_ts = excluded.modified_ts,
                    created_ts = excluded.created_ts,
                    extension = excluded.extension,
                    last_seen_ts = excluded.last_seen_ts
                "#,
                params![
                    entry_path_key,
                    entry.path,
                    entry.parent_path,
                    parent_path_key,
                    entry.name,
                    if entry.is_dir { 1 } else { 0 },
                    entry.size as i64,
                    entry.modified_ts,
                    entry.created_ts,
                    entry.extension,
                    entry.last_seen_ts,
                ],
            )
            .map_err(|error| error.to_string())?;
        }

        for (old_dir_path, old_dir_key) in previous_child_dirs {
            if !inserted_dir_keys.contains(&old_dir_key) {
                let like_pattern = subtree_like_pattern(&old_dir_key);
                tx.execute(
                    "DELETE FROM fs_entries WHERE path_key = ?1 OR path_key LIKE ?2",
                    params![old_dir_key, like_pattern],
                )
                .map_err(|error| error.to_string())?;
                let state_like_pattern = subtree_like_pattern(&normalize_path_key(&old_dir_path));
                tx.execute(
                    "DELETE FROM dir_state WHERE dir_path_key = ?1 OR dir_path_key LIKE ?2",
                    params![
                        normalize_path_key(&old_dir_path),
                        state_like_pattern.clone()
                    ],
                )
                .map_err(|error| error.to_string())?;
                tx.execute(
                    "DELETE FROM file_details_cache WHERE path_key = ?1 OR path_key LIKE ?2",
                    params![normalize_path_key(&old_dir_path), state_like_pattern],
                )
                .map_err(|error| error.to_string())?;
            }
        }

        tx.execute(
            r#"
            INSERT INTO dir_state (dir_path_key, dir_path, is_dirty, last_scan_ts, last_event_ts)
            VALUES (?1, ?2, 0, ?3, ?3)
            ON CONFLICT(dir_path_key) DO UPDATE SET
                dir_path = excluded.dir_path,
                is_dirty = 0,
                last_scan_ts = excluded.last_scan_ts,
                last_event_ts = excluded.last_event_ts
            "#,
            params![dir_key, dir_path, now],
        )
        .map_err(|error| error.to_string())?;

        tx.commit().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn replace_all_entries(
        &self,
        entries: &[FsEntrySnapshot],
        scanned_dirs: &[String],
        scan_timestamp: i64,
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;

        tx.execute("DELETE FROM fs_entries", [])
            .map_err(|error| error.to_string())?;
        tx.execute("DELETE FROM dir_state", [])
            .map_err(|error| error.to_string())?;

        for entry in entries {
            tx.execute(
                r#"
                INSERT INTO fs_entries (
                    path_key, path, parent_path, parent_path_key, name,
                    is_dir, size, modified_ts, created_ts, extension, last_seen_ts
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    normalize_path_key(&entry.path),
                    entry.path,
                    entry.parent_path,
                    normalize_path_key(&entry.parent_path),
                    entry.name,
                    if entry.is_dir { 1 } else { 0 },
                    entry.size as i64,
                    entry.modified_ts,
                    entry.created_ts,
                    entry.extension,
                    entry.last_seen_ts,
                ],
            )
            .map_err(|error| error.to_string())?;
        }

        for dir in scanned_dirs {
            tx.execute(
                r#"
                INSERT INTO dir_state (dir_path_key, dir_path, is_dirty, last_scan_ts, last_event_ts)
                VALUES (?1, ?2, 0, ?3, ?3)
                "#,
                params![normalize_path_key(dir), dir, scan_timestamp],
            )
            .map_err(|error| error.to_string())?;
        }

        tx.execute(
            "UPDATE tree_state SET is_dirty = 0, last_full_scan_ts = ?1 WHERE id = 1",
            params![scan_timestamp],
        )
        .map_err(|error| error.to_string())?;

        tx.commit().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn mark_dir_dirty(&self, dir_path: &str) -> Result<(), String> {
        let now = now_ts();
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute(
            r#"
            INSERT INTO dir_state (dir_path_key, dir_path, is_dirty, last_scan_ts, last_event_ts)
            VALUES (?1, ?2, 1, NULL, ?3)
            ON CONFLICT(dir_path_key) DO UPDATE SET
                dir_path = excluded.dir_path,
                is_dirty = 1,
                last_event_ts = excluded.last_event_ts
            "#,
            params![normalize_path_key(dir_path), dir_path, now],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn clear_dir_dirty(&self, dir_path: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute(
            "UPDATE dir_state SET is_dirty = 0 WHERE dir_path_key = ?1",
            params![normalize_path_key(dir_path)],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn is_dir_dirty(&self, dir_path: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let is_dirty = conn
            .query_row(
                "SELECT is_dirty FROM dir_state WHERE dir_path_key = ?1 LIMIT 1",
                params![normalize_path_key(dir_path)],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .unwrap_or(0);
        Ok(is_dirty == 1)
    }

    pub fn count_dirty_dirs(&self) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let count = conn
            .query_row(
                "SELECT COUNT(*) FROM dir_state WHERE is_dirty = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(count)
    }

    pub fn get_dirty_dirs(&self, limit: usize) -> Result<Vec<String>, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let mut statement = conn
            .prepare(
                r#"
                SELECT dir_path
                FROM dir_state
                WHERE is_dirty = 1
                ORDER BY COALESCE(last_event_ts, 0) ASC
                LIMIT ?1
                "#,
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn mark_tree_dirty(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute("UPDATE tree_state SET is_dirty = 1 WHERE id = 1", [])
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn is_tree_dirty(&self) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let is_dirty = conn
            .query_row("SELECT is_dirty FROM tree_state WHERE id = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .optional()
            .map_err(|error| error.to_string())?
            .unwrap_or(1);
        Ok(is_dirty == 1)
    }

    pub fn has_full_tree_snapshot(&self) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let value: Option<i64> = conn
            .query_row(
                "SELECT last_full_scan_ts FROM tree_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        Ok(value.is_some())
    }

    pub fn set_tree_clean(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute(
            "UPDATE tree_state SET is_dirty = 0, last_full_scan_ts = ?1 WHERE id = 1",
            params![now_ts()],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn remove_path_subtree(&self, path: &str) -> Result<(), String> {
        let key = normalize_path_key(path);
        let like_pattern = subtree_like_pattern(&key);
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute(
            "DELETE FROM fs_entries WHERE path_key = ?1 OR path_key LIKE ?2",
            params![key.clone(), like_pattern.clone()],
        )
        .map_err(|error| error.to_string())?;
        conn.execute(
            "DELETE FROM dir_state WHERE dir_path_key = ?1 OR dir_path_key LIKE ?2",
            params![key.clone(), like_pattern.clone()],
        )
        .map_err(|error| error.to_string())?;
        conn.execute(
            "DELETE FROM file_details_cache WHERE path_key = ?1 OR path_key LIKE ?2",
            params![key, like_pattern],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn invalidate_file_details(&self, file_path: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute(
            "DELETE FROM file_details_cache WHERE path_key = ?1",
            params![normalize_path_key(file_path)],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn invalidate_file_details_by_prefix(&self, path_prefix: &str) -> Result<(), String> {
        let key = normalize_path_key(path_prefix);
        let like_pattern = subtree_like_pattern(&key);
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute(
            "DELETE FROM file_details_cache WHERE path_key = ?1 OR path_key LIKE ?2",
            params![key, like_pattern],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_file_details_payload(
        &self,
        file_path: &str,
        signature: &str,
        now: i64,
    ) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let record: Option<(String, i64)> = conn
            .query_row(
                r#"
                SELECT payload_json, expires_at
                FROM file_details_cache
                WHERE path_key = ?1 AND signature = ?2
                LIMIT 1
                "#,
                params![normalize_path_key(file_path), signature],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        let Some((payload_json, expires_at)) = record else {
            return Ok(None);
        };

        if expires_at <= now {
            conn.execute(
                "DELETE FROM file_details_cache WHERE path_key = ?1",
                params![normalize_path_key(file_path)],
            )
            .map_err(|error| error.to_string())?;
            return Ok(None);
        }

        conn.execute(
            "UPDATE file_details_cache SET last_access_ts = ?2 WHERE path_key = ?1",
            params![normalize_path_key(file_path), now],
        )
        .map_err(|error| error.to_string())?;

        Ok(Some(payload_json))
    }

    pub fn upsert_file_details_payload(
        &self,
        file_path: &str,
        signature: &str,
        payload_json: &str,
        ttl_seconds: i64,
    ) -> Result<(), String> {
        let now = now_ts();
        let expires_at = now + ttl_seconds;
        let path = PathBuf::from(file_path);
        let parent_path = path
            .parent()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());

        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute(
            r#"
            INSERT INTO file_details_cache (
                path_key, path, parent_path, parent_path_key, name,
                signature, payload_json, last_access_ts, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(path_key) DO UPDATE SET
                path = excluded.path,
                parent_path = excluded.parent_path,
                parent_path_key = excluded.parent_path_key,
                name = excluded.name,
                signature = excluded.signature,
                payload_json = excluded.payload_json,
                last_access_ts = excluded.last_access_ts,
                expires_at = excluded.expires_at
            "#,
            params![
                normalize_path_key(file_path),
                file_path,
                parent_path.clone(),
                normalize_path_key(&parent_path),
                name,
                signature,
                payload_json,
                now,
                expires_at,
            ],
        )
        .map_err(|error| error.to_string())?;

        Ok(())
    }

    pub fn cleanup_file_details_cache(
        &self,
        max_entries: usize,
        now_ts: i64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute(
            "DELETE FROM file_details_cache WHERE expires_at <= ?1",
            params![now_ts],
        )
        .map_err(|error| error.to_string())?;

        let count = conn
            .query_row("SELECT COUNT(*) FROM file_details_cache", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|error| error.to_string())?;

        if count > max_entries as i64 {
            let overflow = count - max_entries as i64;
            conn.execute(
                r#"
                DELETE FROM file_details_cache
                WHERE path_key IN (
                    SELECT path_key
                    FROM file_details_cache
                    ORDER BY last_access_ts ASC
                    LIMIT ?1
                )
                "#,
                params![overflow],
            )
            .map_err(|error| error.to_string())?;
        }

        Ok(())
    }
}

fn should_skip_path(project_root: &Path, path: &Path) -> bool {
    if !path.starts_with(project_root) {
        return false;
    }

    match path.strip_prefix(project_root) {
        Ok(relative) => relative.components().any(|component| {
            component
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(".pm_center")
        }),
        Err(_) => false,
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn subtree_like_pattern(path_key: &str) -> String {
    #[cfg(windows)]
    {
        format!("{}\\%", path_key)
    }
    #[cfg(not(windows))]
    {
        format!("{}/%", path_key)
    }
}
