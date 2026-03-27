use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use rusqlite::{Connection, params};
use chrono::DateTime;

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
        
        Self::init_tables(&conn)?;
        
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
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
        let tags = stmt.query_map([], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
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
        let tags = stmt.query_map(params![file_path], |row| {
            row.get::<_, String>(0)
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(tags)
    }
    
    pub fn add_tag_to_file(&self, file_path: &str, tag_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO file_tags (file_path, tag_id) VALUES (?1, ?2)",
            params![file_path, tag_id],
        )?;
        Ok(())
    }
    
    pub fn remove_tag_from_file(&self, file_path: &str, tag_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM file_tags WHERE file_path = ?1 AND tag_id = ?2",
            params![file_path, tag_id],
        )?;
        Ok(())
    }
    
    // ========== 元数据操作 ==========
    
    pub fn get_file_metadata(&self, file_path: &str) -> Result<Option<FileMetadata>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT file_path, status, notes, custom_data FROM file_metadata WHERE file_path = ?1"
        )?;
        
        let result = stmt.query_row(params![file_path], |row| {
            let custom_data_str: Option<String> = row.get(3)?;
            let custom_data = custom_data_str
                .and_then(|s| serde_json::from_str(&s).ok());
            
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
        let custom_data_str = metadata.custom_data.as_ref()
            .map(|v| v.to_string());
        
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

    pub fn move_path_references(&self, old_path: &str, new_path: &str) -> Result<(), rusqlite::Error> {
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
            let mut stmt = tx.prepare("SELECT file_path, status, notes, custom_data FROM file_metadata")?;
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
            })?.collect::<Result<Vec<_>, _>>()?
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
            })?.collect::<Result<Vec<_>, _>>()?
        };
        
        Ok(changes)
    }
    
    pub fn get_change_stats(&self, project_path: &str, since: i64) -> Result<serde_json::Value, rusqlite::Error> {
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
            .as_secs() as i64) - (15 * 24 * 60 * 60);
        
        // 获取需要归档的记录，按日期分组
        let mut stmt = tx.prepare(
            "SELECT id, project_path, file_path, change_type, file_size, timestamp, depth 
             FROM file_changes 
             WHERE timestamp < ?
             ORDER BY timestamp"
        )?;
        
        let old_changes: Vec<FileChange> = stmt.query_map(params![fifteen_days_ago], |row| {
            Ok(FileChange {
                id: row.get(0)?,
                project_path: row.get(1)?,
                file_path: row.get(2)?,
                change_type: row.get(3)?,
                file_size: row.get(4)?,
                timestamp: row.get(5)?,
                depth: row.get(6)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
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
        let datetime = DateTime::from_timestamp(timestamp, 0)
            .unwrap_or_else(|| DateTime::UNIX_EPOCH);
        datetime.format("%Y-%m-%d").to_string()
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
        let changes: Vec<FileChange> = serde_json::from_slice(&data)
            .unwrap_or_default();
        
        Ok(changes)
    }
    
    pub fn get_archived_dates(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT date FROM archived_changes ORDER BY date DESC")?;
        
        let dates = stmt.query_map([], |row| {
            row.get::<_, String>(0)
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(dates)
    }
}
