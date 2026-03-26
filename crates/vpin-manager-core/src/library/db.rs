use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

/// Current schema version. Bump this when adding migrations.
const SCHEMA_VERSION: u32 = 1;

/// A library database instance. Each database file tracks installed resources
/// independently, allowing multiple libraries (e.g., per-cabinet or per-profile).
pub struct LibraryDb {
    conn: Connection,
}

impl LibraryDb {
    /// Open (or create) a library database at the given path.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database (useful for testing).
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Return the default database path for a named library.
    pub fn default_path(name: &str) -> Option<std::path::PathBuf> {
        directories::ProjectDirs::from("", "", "vpin-manager")
            .map(|dirs| dirs.data_dir().join(format!("{name}.db")))
    }

    // --- Migrations ---

    fn migrate(&self) -> Result<(), DbError> {
        let version = self.get_schema_version()?;
        if version < 1 {
            self.migrate_v1()?;
        }
        self.set_schema_version(SCHEMA_VERSION)?;
        Ok(())
    }

    fn get_schema_version(&self) -> Result<u32, DbError> {
        let version: u32 = self.conn.pragma_query_value(None, "user_version", |row| {
            row.get(0)
        })?;
        Ok(version)
    }

    fn set_schema_version(&self, version: u32) -> Result<(), DbError> {
        self.conn
            .pragma_update(None, "user_version", version)?;
        Ok(())
    }

    fn migrate_v1(&self) -> Result<(), DbError> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS installed_resources (
                id              TEXT PRIMARY KEY,
                game_id         TEXT NOT NULL,
                game_name       TEXT NOT NULL,
                resource_type   TEXT NOT NULL,
                version         TEXT,
                file_path       TEXT NOT NULL,
                installed_at    TEXT NOT NULL DEFAULT (datetime('now')),
                vps_updated_at  INTEGER,
                metadata        TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_installed_game_id
                ON installed_resources(game_id);
            CREATE INDEX IF NOT EXISTS idx_installed_resource_type
                ON installed_resources(resource_type);

            CREATE TABLE IF NOT EXISTS download_history (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                resource_id     TEXT NOT NULL,
                game_id         TEXT NOT NULL,
                url             TEXT NOT NULL,
                status          TEXT NOT NULL,
                started_at      TEXT NOT NULL DEFAULT (datetime('now')),
                completed_at    TEXT,
                file_path       TEXT,
                error           TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_download_resource_id
                ON download_history(resource_id);
            CREATE INDEX IF NOT EXISTS idx_download_game_id
                ON download_history(game_id);
            ",
        )?;
        Ok(())
    }

    // --- Installed Resources ---

    /// Insert or replace an installed resource.
    pub fn upsert_installed(&self, resource: &InstalledResource) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO installed_resources
                (id, game_id, game_name, resource_type, version, file_path, installed_at, vps_updated_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), ?7, ?8)",
            params![
                resource.id,
                resource.game_id,
                resource.game_name,
                resource.resource_type,
                resource.version,
                resource.file_path,
                resource.vps_updated_at,
                resource.metadata,
            ],
        )?;
        Ok(())
    }

    /// Get an installed resource by VPS resource ID.
    pub fn get_installed(&self, id: &str) -> Result<Option<InstalledResource>, DbError> {
        let result = self
            .conn
            .query_row(
                "SELECT id, game_id, game_name, resource_type, version, file_path,
                        installed_at, vps_updated_at, metadata
                 FROM installed_resources WHERE id = ?1",
                params![id],
                |row| {
                    Ok(InstalledResource {
                        id: row.get(0)?,
                        game_id: row.get(1)?,
                        game_name: row.get(2)?,
                        resource_type: row.get(3)?,
                        version: row.get(4)?,
                        file_path: row.get(5)?,
                        installed_at: row.get(6)?,
                        vps_updated_at: row.get(7)?,
                        metadata: row.get(8)?,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    /// List all installed resources, optionally filtered by game ID.
    pub fn list_installed(
        &self,
        game_id: Option<&str>,
    ) -> Result<Vec<InstalledResource>, DbError> {
        let mut resources = Vec::new();

        if let Some(gid) = game_id {
            let mut stmt = self.conn.prepare(
                "SELECT id, game_id, game_name, resource_type, version, file_path,
                        installed_at, vps_updated_at, metadata
                 FROM installed_resources WHERE game_id = ?1
                 ORDER BY game_name, resource_type",
            )?;
            let rows = stmt.query_map(params![gid], row_to_installed)?;
            for row in rows {
                resources.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, game_id, game_name, resource_type, version, file_path,
                        installed_at, vps_updated_at, metadata
                 FROM installed_resources
                 ORDER BY game_name, resource_type",
            )?;
            let rows = stmt.query_map([], row_to_installed)?;
            for row in rows {
                resources.push(row?);
            }
        }

        Ok(resources)
    }

    /// Remove an installed resource by ID.
    pub fn remove_installed(&self, id: &str) -> Result<bool, DbError> {
        let count = self.conn.execute(
            "DELETE FROM installed_resources WHERE id = ?1",
            params![id],
        )?;
        Ok(count > 0)
    }

    /// Count installed resources.
    pub fn count_installed(&self) -> Result<u64, DbError> {
        let count: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM installed_resources",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// List distinct game IDs that have installed resources.
    pub fn installed_game_ids(&self) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT game_id FROM installed_resources ORDER BY game_id")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        Ok(ids)
    }

    // --- Download History ---

    /// Record a download attempt.
    pub fn record_download(&self, record: &DownloadRecord) -> Result<i64, DbError> {
        self.conn.execute(
            "INSERT INTO download_history
                (resource_id, game_id, url, status, started_at, completed_at, file_path, error)
             VALUES (?1, ?2, ?3, ?4, datetime('now'), ?5, ?6, ?7)",
            params![
                record.resource_id,
                record.game_id,
                record.url,
                record.status,
                record.completed_at,
                record.file_path,
                record.error,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Update a download record's status.
    pub fn update_download_status(
        &self,
        id: i64,
        status: &str,
        completed_at: Option<&str>,
        file_path: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE download_history
             SET status = ?1, completed_at = ?2, file_path = ?3, error = ?4
             WHERE id = ?5",
            params![status, completed_at, file_path, error, id],
        )?;
        Ok(())
    }

    /// List download history for a resource.
    pub fn download_history_for(
        &self,
        resource_id: &str,
    ) -> Result<Vec<DownloadRecord>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, resource_id, game_id, url, status, started_at,
                    completed_at, file_path, error
             FROM download_history WHERE resource_id = ?1
             ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map(params![resource_id], row_to_download)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
}

fn row_to_installed(row: &rusqlite::Row) -> rusqlite::Result<InstalledResource> {
    Ok(InstalledResource {
        id: row.get(0)?,
        game_id: row.get(1)?,
        game_name: row.get(2)?,
        resource_type: row.get(3)?,
        version: row.get(4)?,
        file_path: row.get(5)?,
        installed_at: row.get(6)?,
        vps_updated_at: row.get(7)?,
        metadata: row.get(8)?,
    })
}

fn row_to_download(row: &rusqlite::Row) -> rusqlite::Result<DownloadRecord> {
    Ok(DownloadRecord {
        id: Some(row.get(0)?),
        resource_id: row.get(1)?,
        game_id: row.get(2)?,
        url: row.get(3)?,
        status: row.get(4)?,
        started_at: row.get(5)?,
        completed_at: row.get(6)?,
        file_path: row.get(7)?,
        error: row.get(8)?,
    })
}

// --- Data types ---

#[derive(Debug, Clone)]
pub struct InstalledResource {
    pub id: String,
    pub game_id: String,
    pub game_name: String,
    pub resource_type: String,
    pub version: Option<String>,
    pub file_path: String,
    pub installed_at: Option<String>,
    pub vps_updated_at: Option<i64>,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DownloadRecord {
    pub id: Option<i64>,
    pub resource_id: String,
    pub game_id: String,
    pub url: String,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub file_path: Option<String>,
    pub error: Option<String>,
}

// --- Errors ---

#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Sqlite(e) => write!(f, "database error: {e}"),
            DbError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sqlite(e)
    }
}

impl From<std::io::Error> for DbError {
    fn from(e: std::io::Error) -> Self {
        DbError::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> LibraryDb {
        LibraryDb::open_in_memory().unwrap()
    }

    fn sample_resource(id: &str, game_id: &str) -> InstalledResource {
        InstalledResource {
            id: id.to_string(),
            game_id: game_id.to_string(),
            game_name: "Test Game".to_string(),
            resource_type: "tables".to_string(),
            version: Some("1.0".to_string()),
            file_path: "/path/to/file.vpx".to_string(),
            installed_at: None,
            vps_updated_at: Some(1745823539995),
            metadata: None,
        }
    }

    #[test]
    fn create_and_migrate() {
        let db = test_db();
        assert_eq!(db.get_schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn upsert_and_get() {
        let db = test_db();
        let res = sample_resource("r1", "g1");
        db.upsert_installed(&res).unwrap();

        let fetched = db.get_installed("r1").unwrap().unwrap();
        assert_eq!(fetched.game_id, "g1");
        assert_eq!(fetched.game_name, "Test Game");
        assert_eq!(fetched.resource_type, "tables");
        assert_eq!(fetched.version.as_deref(), Some("1.0"));
        assert!(fetched.installed_at.is_some());
    }

    #[test]
    fn upsert_replaces() {
        let db = test_db();
        let mut res = sample_resource("r1", "g1");
        db.upsert_installed(&res).unwrap();

        res.version = Some("2.0".to_string());
        db.upsert_installed(&res).unwrap();

        let fetched = db.get_installed("r1").unwrap().unwrap();
        assert_eq!(fetched.version.as_deref(), Some("2.0"));
        assert_eq!(db.count_installed().unwrap(), 1);
    }

    #[test]
    fn list_all_and_by_game() {
        let db = test_db();
        db.upsert_installed(&sample_resource("r1", "g1")).unwrap();
        db.upsert_installed(&sample_resource("r2", "g1")).unwrap();
        db.upsert_installed(&sample_resource("r3", "g2")).unwrap();

        let all = db.list_installed(None).unwrap();
        assert_eq!(all.len(), 3);

        let g1 = db.list_installed(Some("g1")).unwrap();
        assert_eq!(g1.len(), 2);

        let g2 = db.list_installed(Some("g2")).unwrap();
        assert_eq!(g2.len(), 1);
    }

    #[test]
    fn remove_installed() {
        let db = test_db();
        db.upsert_installed(&sample_resource("r1", "g1")).unwrap();
        assert!(db.remove_installed("r1").unwrap());
        assert!(!db.remove_installed("r1").unwrap());
        assert_eq!(db.count_installed().unwrap(), 0);
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let db = test_db();
        assert!(db.get_installed("nope").unwrap().is_none());
    }

    #[test]
    fn installed_game_ids() {
        let db = test_db();
        db.upsert_installed(&sample_resource("r1", "g1")).unwrap();
        db.upsert_installed(&sample_resource("r2", "g2")).unwrap();
        db.upsert_installed(&sample_resource("r3", "g1")).unwrap();

        let ids = db.installed_game_ids().unwrap();
        assert_eq!(ids, vec!["g1", "g2"]);
    }

    #[test]
    fn download_history() {
        let db = test_db();
        let record = DownloadRecord {
            id: None,
            resource_id: "r1".to_string(),
            game_id: "g1".to_string(),
            url: "https://example.com/file.zip".to_string(),
            status: "started".to_string(),
            started_at: None,
            completed_at: None,
            file_path: None,
            error: None,
        };

        let row_id = db.record_download(&record).unwrap();
        assert!(row_id > 0);

        db.update_download_status(
            row_id,
            "completed",
            Some("2026-03-26T12:00:00"),
            Some("/path/to/file.zip"),
            None,
        )
        .unwrap();

        let history = db.download_history_for("r1").unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, "completed");
        assert_eq!(
            history[0].file_path.as_deref(),
            Some("/path/to/file.zip")
        );
    }

    #[test]
    fn multiple_databases_independent() {
        let db1 = test_db();
        let db2 = test_db();

        db1.upsert_installed(&sample_resource("r1", "g1")).unwrap();

        assert_eq!(db1.count_installed().unwrap(), 1);
        assert_eq!(db2.count_installed().unwrap(), 0);
    }
}
