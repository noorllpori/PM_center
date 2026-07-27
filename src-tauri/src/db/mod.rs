use chrono::DateTime;
use rusqlite::{params, params_from_iter, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn replace_path_prefix(path: &str, old_path: &str, new_path: &str) -> Option<String> {
    if path == old_path {
        return Some(new_path.to_string());
    }

    let separator = std::path::MAIN_SEPARATOR;
    let prefix = format!("{}{}", old_path, separator);

    if path.starts_with(&prefix) {
        return Some(format!("{}{}", new_path, &path[old_path.len()..]));
    }

    None
}

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub file_path: String,
    pub status: Option<String>,
    pub notes: Option<String>,
    pub custom_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub directory_path: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub item_count: i64,
    pub member_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionMemberUpdate {
    pub collection_id: String,
    pub added_count: i64,
    pub already_present_count: i64,
    pub item_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionMemberRemoval {
    pub collection_id: String,
    pub removed_count: i64,
    pub not_found_count: i64,
    pub item_count: i64,
}

// 文件变更记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub id: i64,
    pub project_path: String,
    pub file_path: String,
    pub change_type: String, // created, modified, deleted
    pub file_size: Option<i64>,
    pub timestamp: i64,
    pub depth: i32,
}

// 归档的变更记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedChange {
    pub id: i64,
    pub date: String, // YYYY-MM-DD
    pub compressed_data: Vec<u8>,
    pub record_count: i32,
}

impl Database {
    pub fn new(project_path: &str) -> Result<Self, rusqlite::Error> {
        let data_dir = PathBuf::from(project_path).join(".pm_center");
        std::fs::create_dir_all(&data_dir).ok();

        let db_path = data_dir.join("data.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        Self::init_tables(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn init_tables(conn: &Connection) -> Result<(), rusqlite::Error> {
        // 标签表
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT '#1890ff'
            )
            "#,
            [],
        )?;

        // 文件标签表
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS file_tags (
                file_path TEXT NOT NULL,
                tag_id TEXT NOT NULL,
                PRIMARY KEY (file_path, tag_id),
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )
            "#,
            [],
        )?;

        // 文件元数据表
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS file_metadata (
                file_path TEXT PRIMARY KEY,
                status TEXT,
                notes TEXT,
                custom_data TEXT
            )
            "#,
            [],
        )?;

        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                directory_path TEXT NOT NULL,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(directory_path, name)
            )
            "#,
            [],
        )?;

        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS collection_items (
                collection_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                position INTEGER NOT NULL,
                PRIMARY KEY(collection_id, file_path),
                FOREIGN KEY(collection_id) REFERENCES collections(id) ON DELETE CASCADE
            )
            "#,
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_collections_directory ON collections(directory_path)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_collection_items_collection ON collection_items(collection_id, position)",
            [],
        )?;

        // 文件变更日志表（最近15天）
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS file_changes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_path TEXT NOT NULL,
                file_path TEXT NOT NULL,
                change_type TEXT NOT NULL,
                file_size INTEGER,
                timestamp INTEGER NOT NULL,
                depth INTEGER DEFAULT 0
            )
            "#,
            [],
        )?;

        // 创建索引
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_changes_project_time ON file_changes(project_path, timestamp)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_changes_time ON file_changes(timestamp)",
            [],
        )?;

        // 归档表（压缩存储）
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS archived_changes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL UNIQUE,
                compressed_data BLOB NOT NULL,
                record_count INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            )
            "#,
            [],
        )?;

        // 插入默认标签
        let default_tags = vec![
            ("wip", "进行中", "#faad14"),
            ("review", "待审核", "#1890ff"),
            ("approved", "已通过", "#52c41a"),
            ("final", "最终版", "#722ed1"),
        ];

        for (id, name, color) in default_tags {
            conn.execute(
                "INSERT OR IGNORE INTO tags (id, name, color) VALUES (?1, ?2, ?3)",
                params![id, name, color],
            )?;
        }

        Ok(())
    }

    // ========== 标签操作 ==========

    pub fn get_all_tags(&self) -> Result<Vec<Tag>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, color FROM tags")?;
        let tags = stmt
            .query_map([], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tags)
    }

    pub fn add_tag(&self, id: &str, name: &str, color: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tags (id, name, color) VALUES (?1, ?2, ?3)",
            params![id, name, color],
        )?;
        Ok(())
    }

    pub fn delete_tag(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ========== 文件标签操作 ==========

    pub fn get_file_tags(&self, file_path: &str) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT tag_id FROM file_tags WHERE file_path = ?1")?;
        let tags = stmt
            .query_map(params![file_path], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tags)
    }

    pub fn get_file_tags_batch(
        &self,
        file_paths: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<String>>, rusqlite::Error> {
        let mut result = std::collections::HashMap::new();
        if file_paths.is_empty() {
            return Ok(result);
        }

        let conn = self.conn.lock().unwrap();
        let placeholders = std::iter::repeat("?")
            .take(file_paths.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT file_path, tag_id FROM file_tags WHERE file_path IN ({})",
            placeholders
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(file_paths.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (file_path, tag_id) = row?;
            result
                .entry(file_path)
                .or_insert_with(Vec::new)
                .push(tag_id);
        }

        Ok(result)
    }

    pub fn add_tag_to_file(&self, file_path: &str, tag_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO file_tags (file_path, tag_id) VALUES (?1, ?2)",
            params![file_path, tag_id],
        )?;
        Ok(())
    }

    pub fn remove_tag_from_file(
        &self,
        file_path: &str,
        tag_id: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM file_tags WHERE file_path = ?1 AND tag_id = ?2",
            params![file_path, tag_id],
        )?;
        Ok(())
    }

    // ========== 元数据操作 ==========

    pub fn get_file_metadata(
        &self,
        file_path: &str,
    ) -> Result<Option<FileMetadata>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT file_path, status, notes, custom_data FROM file_metadata WHERE file_path = ?1",
        )?;

        let result = stmt.query_row(params![file_path], |row| {
            let custom_data_str: Option<String> = row.get(3)?;
            let custom_data = custom_data_str.and_then(|s| serde_json::from_str(&s).ok());

            Ok(FileMetadata {
                file_path: row.get(0)?,
                status: row.get(1)?,
                notes: row.get(2)?,
                custom_data,
            })
        });

        match result {
            Ok(meta) => Ok(Some(meta)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn update_file_metadata(&self, metadata: &FileMetadata) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let custom_data_str = metadata.custom_data.as_ref().map(|v| v.to_string());

        conn.execute(
            r#"
            INSERT INTO file_metadata (file_path, status, notes, custom_data)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(file_path) DO UPDATE SET
                status = excluded.status,
                notes = excluded.notes,
                custom_data = excluded.custom_data
            "#,
            params![
                metadata.file_path,
                metadata.status,
                metadata.notes,
                custom_data_str,
            ],
        )?;

        Ok(())
    }

    pub fn create_collection(
        &self,
        directory_path: &str,
        name: &str,
        member_paths: &[String],
    ) -> Result<Collection, rusqlite::Error> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "collection name is required".to_string(),
            ));
        }

        let mut unique_member_paths = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();
        for member_path in member_paths {
            let member_path = member_path.trim();
            if member_path.is_empty() || !seen_paths.insert(member_path.to_string()) {
                continue;
            }
            unique_member_paths.push(member_path.to_string());
        }

        if unique_member_paths.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "collection members are required".to_string(),
            ));
        }

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let now = Self::current_timestamp();
        let id = Uuid::new_v4().to_string();

        tx.execute(
            r#"
            INSERT INTO collections (id, directory_path, name, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![id, directory_path, trimmed_name, now, now],
        )?;

        for (position, member_path) in unique_member_paths.iter().enumerate() {
            tx.execute(
                r#"
                INSERT INTO collection_items (collection_id, file_path, position)
                VALUES (?1, ?2, ?3)
                "#,
                params![id, member_path, position as i64],
            )?;
        }

        tx.commit()?;

        Ok(Collection {
            id,
            directory_path: directory_path.to_string(),
            name: trimmed_name.to_string(),
            created_at: now,
            updated_at: now,
            item_count: unique_member_paths.len() as i64,
            member_paths: unique_member_paths,
        })
    }

    pub fn list_all_collections(&self) -> Result<Vec<Collection>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT c.id, c.directory_path, c.name, c.created_at, c.updated_at,
                   COUNT(ci.file_path) AS item_count
            FROM collections c
            LEFT JOIN collection_items ci ON ci.collection_id = c.id
            GROUP BY c.id, c.directory_path, c.name, c.created_at, c.updated_at
            ORDER BY lower(c.name)
            "#,
        )?;

        let mut collections = stmt
            .query_map([], |row| {
                Ok(Collection {
                    id: row.get(0)?,
                    directory_path: row.get(1)?,
                    name: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    item_count: row.get(5)?,
                    member_paths: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        drop(stmt);

        for collection in &mut collections {
            collection.member_paths =
                Self::get_collection_item_paths_with_conn(&conn, &collection.id)?;
        }

        Ok(collections)
    }

    pub fn get_collection_item_paths(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        Self::get_collection_item_paths_with_conn(&conn, collection_id)
    }

    pub fn add_collection_items(
        &self,
        collection_id: &str,
        member_paths: &[String],
    ) -> Result<CollectionMemberUpdate, rusqlite::Error> {
        let mut unique_member_paths = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();
        for member_path in member_paths {
            let member_path = member_path.trim();
            if member_path.is_empty() || !seen_paths.insert(member_path.to_string()) {
                continue;
            }
            unique_member_paths.push(member_path.to_string());
        }

        if unique_member_paths.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "collection members are required".to_string(),
            ));
        }

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.query_row(
            "SELECT id FROM collections WHERE id = ?1",
            params![collection_id],
            |row| row.get::<_, String>(0),
        )?;

        let mut next_position = tx.query_row(
            "SELECT COALESCE(MAX(position), -1) FROM collection_items WHERE collection_id = ?1",
            params![collection_id],
            |row| row.get::<_, i64>(0),
        )? + 1;
        let mut added_count = 0_i64;
        let mut already_present_count = 0_i64;

        for member_path in unique_member_paths {
            let inserted = tx.execute(
                r#"
                INSERT OR IGNORE INTO collection_items (collection_id, file_path, position)
                VALUES (?1, ?2, ?3)
                "#,
                params![collection_id, member_path, next_position],
            )?;

            if inserted > 0 {
                added_count += 1;
                next_position += 1;
            } else {
                already_present_count += 1;
            }
        }

        if added_count > 0 {
            tx.execute(
                "UPDATE collections SET updated_at = ?1 WHERE id = ?2",
                params![Self::current_timestamp(), collection_id],
            )?;
        }

        let item_count = tx.query_row(
            "SELECT COUNT(*) FROM collection_items WHERE collection_id = ?1",
            params![collection_id],
            |row| row.get::<_, i64>(0),
        )?;
        tx.commit()?;

        Ok(CollectionMemberUpdate {
            collection_id: collection_id.to_string(),
            added_count,
            already_present_count,
            item_count,
        })
    }

    pub fn remove_collection_items(
        &self,
        collection_id: &str,
        member_paths: &[String],
    ) -> Result<CollectionMemberRemoval, rusqlite::Error> {
        let mut unique_member_paths = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();
        for member_path in member_paths {
            let member_path = member_path.trim();
            if member_path.is_empty() || !seen_paths.insert(member_path.to_string()) {
                continue;
            }
            unique_member_paths.push(member_path.to_string());
        }

        if unique_member_paths.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "collection members are required".to_string(),
            ));
        }

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.query_row(
            "SELECT id FROM collections WHERE id = ?1",
            params![collection_id],
            |row| row.get::<_, String>(0),
        )?;

        let mut removed_count = 0_i64;
        let mut not_found_count = 0_i64;
        for member_path in unique_member_paths {
            let deleted = tx.execute(
                "DELETE FROM collection_items WHERE collection_id = ?1 AND file_path = ?2",
                params![collection_id, member_path],
            )?;
            if deleted > 0 {
                removed_count += 1;
            } else {
                not_found_count += 1;
            }
        }

        if removed_count > 0 {
            tx.execute(
                "UPDATE collections SET updated_at = ?1 WHERE id = ?2",
                params![Self::current_timestamp(), collection_id],
            )?;
        }

        let item_count = tx.query_row(
            "SELECT COUNT(*) FROM collection_items WHERE collection_id = ?1",
            params![collection_id],
            |row| row.get::<_, i64>(0),
        )?;
        tx.commit()?;

        Ok(CollectionMemberRemoval {
            collection_id: collection_id.to_string(),
            removed_count,
            not_found_count,
            item_count,
        })
    }

    pub fn rename_collection(
        &self,
        collection_id: &str,
        name: &str,
    ) -> Result<(), rusqlite::Error> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "collection name is required".to_string(),
            ));
        }

        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            UPDATE collections
            SET name = ?1, updated_at = ?2
            WHERE id = ?3
            "#,
            params![trimmed_name, Self::current_timestamp(), collection_id],
        )?;
        Ok(())
    }

    pub fn delete_collection(&self, collection_id: &str) -> Result<(), rusqlite::Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM collection_items WHERE collection_id = ?1",
            params![collection_id],
        )?;
        tx.execute(
            "DELETE FROM collections WHERE id = ?1",
            params![collection_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn get_collection_item_paths_with_conn(
        conn: &Connection,
        collection_id: &str,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            r#"
            SELECT file_path
            FROM collection_items
            WHERE collection_id = ?1
            ORDER BY position ASC, file_path ASC
            "#,
        )?;

        let paths = stmt
            .query_map(params![collection_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(paths)
    }

    pub fn move_path_references(
        &self,
        old_path: &str,
        new_path: &str,
    ) -> Result<(), rusqlite::Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // 迁移文件标签
        let tag_rows: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT file_path, tag_id FROM file_tags")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        for (file_path, tag_id) in tag_rows {
            if let Some(updated_path) = replace_path_prefix(&file_path, old_path, new_path) {
                tx.execute(
                    "DELETE FROM file_tags WHERE file_path = ?1 AND tag_id = ?2",
                    params![file_path, tag_id],
                )?;
                tx.execute(
                    "INSERT OR IGNORE INTO file_tags (file_path, tag_id) VALUES (?1, ?2)",
                    params![updated_path, tag_id],
                )?;
            }
        }

        // 迁移文件元数据
        let metadata_rows: Vec<(String, Option<String>, Option<String>, Option<String>)> = {
            let mut stmt =
                tx.prepare("SELECT file_path, status, notes, custom_data FROM file_metadata")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        for (file_path, status, notes, custom_data) in metadata_rows {
            if let Some(updated_path) = replace_path_prefix(&file_path, old_path, new_path) {
                tx.execute(
                    "DELETE FROM file_metadata WHERE file_path = ?1",
                    params![file_path],
                )?;
                tx.execute(
                    r#"
                    INSERT OR REPLACE INTO file_metadata (file_path, status, notes, custom_data)
                    VALUES (?1, ?2, ?3, ?4)
                    "#,
                    params![updated_path, status, notes, custom_data],
                )?;
            }
        }

        let collection_rows: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT id, directory_path FROM collections")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        for (id, directory_path) in collection_rows {
            if let Some(updated_path) = replace_path_prefix(&directory_path, old_path, new_path) {
                tx.execute(
                    "UPDATE collections SET directory_path = ?1, updated_at = ?2 WHERE id = ?3",
                    params![updated_path, Self::current_timestamp(), id],
                )?;
            }
        }

        let collection_item_rows: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT collection_id, file_path FROM collection_items")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        for (collection_id, file_path) in collection_item_rows {
            if let Some(updated_path) = replace_path_prefix(&file_path, old_path, new_path) {
                tx.execute(
                    "UPDATE OR IGNORE collection_items SET file_path = ?1 WHERE collection_id = ?2 AND file_path = ?3",
                    params![updated_path, collection_id, file_path],
                )?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    // ========== 文件变更日志操作 ==========

    pub fn add_file_change(&self, change: &FileChange) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO file_changes (project_path, file_path, change_type, file_size, timestamp, depth)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                change.project_path,
                change.file_path,
                change.change_type,
                change.file_size,
                change.timestamp,
                change.depth,
            ],
        )?;
        Ok(())
    }

    pub fn add_file_changes_batch(&self, changes: &[FileChange]) -> Result<usize, rusqlite::Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let mut count = 0;
        for change in changes {
            tx.execute(
                r#"
                INSERT INTO file_changes (project_path, file_path, change_type, file_size, timestamp, depth)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    change.project_path,
                    change.file_path,
                    change.change_type,
                    change.file_size,
                    change.timestamp,
                    change.depth,
                ],
            )?;
            count += 1;
        }

        tx.commit()?;
        Ok(count)
    }

    pub fn get_file_changes(
        &self,
        project_path: &str,
        since: i64,
        change_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<FileChange>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        let sql = if change_type.is_some() {
            "SELECT id, project_path, file_path, change_type, file_size, timestamp, depth 
             FROM file_changes 
             WHERE project_path = ? AND timestamp > ? AND change_type = ?
             ORDER BY timestamp DESC
             LIMIT ?"
        } else {
            "SELECT id, project_path, file_path, change_type, file_size, timestamp, depth 
             FROM file_changes 
             WHERE project_path = ? AND timestamp > ?
             ORDER BY timestamp DESC
             LIMIT ?"
        };

        let mut stmt = conn.prepare(sql)?;

        let changes = if let Some(ct) = change_type {
            stmt.query_map(params![project_path, since, ct, limit], |row| {
                Ok(FileChange {
                    id: row.get(0)?,
                    project_path: row.get(1)?,
                    file_path: row.get(2)?,
                    change_type: row.get(3)?,
                    file_size: row.get(4)?,
                    timestamp: row.get(5)?,
                    depth: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(params![project_path, since, limit], |row| {
                Ok(FileChange {
                    id: row.get(0)?,
                    project_path: row.get(1)?,
                    file_path: row.get(2)?,
                    change_type: row.get(3)?,
                    file_size: row.get(4)?,
                    timestamp: row.get(5)?,
                    depth: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
        };

        Ok(changes)
    }

    pub fn get_change_stats(
        &self,
        project_path: &str,
        since: i64,
    ) -> Result<serde_json::Value, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_changes WHERE project_path = ? AND timestamp > ?",
            params![project_path, since],
            |row| row.get(0),
        )?;

        let created: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_changes WHERE project_path = ? AND timestamp > ? AND change_type = 'created'",
            params![project_path, since],
            |row| row.get(0),
        )?;

        let modified: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_changes WHERE project_path = ? AND timestamp > ? AND change_type = 'modified'",
            params![project_path, since],
            |row| row.get(0),
        )?;

        let deleted: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_changes WHERE project_path = ? AND timestamp > ? AND change_type = 'deleted'",
            params![project_path, since],
            |row| row.get(0),
        )?;

        Ok(serde_json::json!({
            "total": total,
            "created": created,
            "modified": modified,
            "deleted": deleted,
        }))
    }

    // ========== 归档操作 ==========

    pub fn archive_old_changes(&self) -> Result<usize, rusqlite::Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // 计算15天前的时间戳
        let fifteen_days_ago = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64)
            - (15 * 24 * 60 * 60);

        // 获取需要归档的记录，按日期分组
        let mut stmt = tx.prepare(
            "SELECT id, project_path, file_path, change_type, file_size, timestamp, depth 
             FROM file_changes 
             WHERE timestamp < ?
             ORDER BY timestamp",
        )?;

        let old_changes: Vec<FileChange> = stmt
            .query_map(params![fifteen_days_ago], |row| {
                Ok(FileChange {
                    id: row.get(0)?,
                    project_path: row.get(1)?,
                    file_path: row.get(2)?,
                    change_type: row.get(3)?,
                    file_size: row.get(4)?,
                    timestamp: row.get(5)?,
                    depth: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        drop(stmt);

        if old_changes.is_empty() {
            return Ok(0);
        }

        // 按日期分组
        use std::collections::HashMap;
        let mut by_date: HashMap<String, Vec<FileChange>> = HashMap::new();

        for change in old_changes {
            let date = Self::timestamp_to_date(change.timestamp);
            by_date.entry(date).or_default().push(change);
        }

        // 压缩并保存每一天的数据
        for (date, changes) in by_date {
            // 序列化为JSON
            let json_data = serde_json::to_vec(&changes).unwrap_or_default();

            // 使用简单压缩（这里用JSON，实际可以用gzip等）
            // TODO: 添加gzip压缩
            let compressed = json_data;

            tx.execute(
                r#"
                INSERT INTO archived_changes (date, compressed_data, record_count, created_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(date) DO UPDATE SET
                    compressed_data = excluded.compressed_data,
                    record_count = excluded.record_count + archived_changes.record_count,
                    created_at = excluded.created_at
                "#,
                params![
                    date,
                    compressed,
                    changes.len() as i32,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                ],
            )?;
        }

        // 删除已归档的记录
        let deleted = tx.execute(
            "DELETE FROM file_changes WHERE timestamp < ?",
            params![fifteen_days_ago],
        )?;

        tx.commit()?;
        Ok(deleted)
    }

    fn timestamp_to_date(timestamp: i64) -> String {
        let datetime =
            DateTime::from_timestamp(timestamp, 0).unwrap_or_else(|| DateTime::UNIX_EPOCH);
        datetime.format("%Y-%m-%d").to_string()
    }

    fn current_timestamp() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default()
    }

    // 获取归档的变更记录
    pub fn get_archived_changes(&self, date: &str) -> Result<Vec<FileChange>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        let data: Vec<u8> = conn.query_row(
            "SELECT compressed_data FROM archived_changes WHERE date = ?",
            params![date],
            |row| row.get(0),
        )?;

        // 解压缩（目前只是JSON反序列化）
        let changes: Vec<FileChange> = serde_json::from_slice(&data).unwrap_or_default();

        Ok(changes)
    }

    pub fn get_archived_dates(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT date FROM archived_changes ORDER BY date DESC")?;

        let dates = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(dates)
    }
}

#[cfg(test)]
mod tests {
    use super::Database;

    fn make_temp_project_path() -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pm-center-db-test-{}", nonce))
    }

    #[test]
    fn adding_collection_items_appends_new_members_without_duplicates() {
        let project_path = make_temp_project_path();
        std::fs::create_dir_all(&project_path).unwrap();
        let database = Database::new(&project_path.to_string_lossy()).unwrap();
        let collection = database
            .create_collection(
                "C:\\project",
                "镜头",
                &[
                    "C:\\project\\a.png".to_string(),
                    "C:\\project\\b.png".to_string(),
                ],
            )
            .unwrap();

        let update = database
            .add_collection_items(
                &collection.id,
                &[
                    "C:\\project\\b.png".to_string(),
                    "C:\\project\\c.png".to_string(),
                    "C:\\project\\c.png".to_string(),
                ],
            )
            .unwrap();

        assert_eq!(update.added_count, 1);
        assert_eq!(update.already_present_count, 1);
        assert_eq!(update.item_count, 3);
        assert_eq!(
            database.get_collection_item_paths(&collection.id).unwrap(),
            vec![
                "C:\\project\\a.png".to_string(),
                "C:\\project\\b.png".to_string(),
                "C:\\project\\c.png".to_string(),
            ],
        );

        drop(database);
        std::fs::remove_dir_all(project_path).unwrap();
    }

    #[test]
    fn removing_collection_items_only_removes_collection_references() {
        let project_path = make_temp_project_path();
        std::fs::create_dir_all(&project_path).unwrap();
        let database = Database::new(&project_path.to_string_lossy()).unwrap();
        let collection = database
            .create_collection(
                "C:\\project",
                "镜头",
                &[
                    "C:\\project\\a.png".to_string(),
                    "C:\\project\\b.png".to_string(),
                ],
            )
            .unwrap();

        let removal = database
            .remove_collection_items(
                &collection.id,
                &[
                    "C:\\project\\b.png".to_string(),
                    "C:\\project\\missing.png".to_string(),
                    "C:\\project\\b.png".to_string(),
                ],
            )
            .unwrap();

        assert_eq!(removal.removed_count, 1);
        assert_eq!(removal.not_found_count, 1);
        assert_eq!(removal.item_count, 1);
        assert_eq!(
            database.get_collection_item_paths(&collection.id).unwrap(),
            vec!["C:\\project\\a.png".to_string()],
        );

        drop(database);
        std::fs::remove_dir_all(project_path).unwrap();
    }
}
