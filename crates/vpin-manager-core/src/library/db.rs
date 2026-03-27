use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

/// Current schema version. Bump this when adding migrations.
const SCHEMA_VERSION: u32 = 2;

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
        if version < 2 {
            self.migrate_v2()?;
        }
        self.set_schema_version(SCHEMA_VERSION)?;
        Ok(())
    }

    fn get_schema_version(&self) -> Result<u32, DbError> {
        let version: u32 = self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        Ok(version)
    }

    fn set_schema_version(&self, version: u32) -> Result<(), DbError> {
        self.conn.pragma_update(None, "user_version", version)?;
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

    fn migrate_v2(&self) -> Result<(), DbError> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS authors (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                name            TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS resource_authors (
                resource_id     TEXT NOT NULL REFERENCES installed_resources(id) ON DELETE CASCADE,
                author_id       INTEGER NOT NULL REFERENCES authors(id) ON DELETE CASCADE,
                PRIMARY KEY (resource_id, author_id)
            );

            CREATE INDEX IF NOT EXISTS idx_resource_authors_resource
                ON resource_authors(resource_id);
            CREATE INDEX IF NOT EXISTS idx_resource_authors_author
                ON resource_authors(author_id);
            ",
        )?;
        Ok(())
    }

    // --- Installed Resources ---

    /// Insert or replace an installed resource, including its authors.
    /// If a resource with the same file_path already exists, it is updated
    /// (even if the ID differs) to prevent duplicates.
    pub fn upsert_installed(&self, resource: &InstalledResource) -> Result<(), DbError> {
        let tx = self.conn.unchecked_transaction()?;

        // Remove any existing entry with the same file_path but different ID
        let existing_id: Option<String> = tx
            .query_row(
                "SELECT id FROM installed_resources WHERE file_path = ?1",
                params![resource.file_path],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(ref old_id) = existing_id
            && old_id != &resource.id
        {
            tx.execute(
                "DELETE FROM installed_resources WHERE id = ?1",
                params![old_id],
            )?;
        }

        tx.execute(
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

        // Clear existing author associations and re-insert
        tx.execute(
            "DELETE FROM resource_authors WHERE resource_id = ?1",
            params![resource.id],
        )?;

        for author_name in &resource.authors {
            tx.execute(
                "INSERT OR IGNORE INTO authors (name) VALUES (?1)",
                params![author_name],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO resource_authors (resource_id, author_id)
                 SELECT ?1, id FROM authors WHERE name = ?2",
                params![resource.id, author_name],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Get an installed resource by VPS resource ID, including authors.
    pub fn get_installed(&self, id: &str) -> Result<Option<InstalledResource>, DbError> {
        let result = self
            .conn
            .query_row(
                "SELECT id, game_id, game_name, resource_type, version, file_path,
                        installed_at, vps_updated_at, metadata
                 FROM installed_resources WHERE id = ?1",
                params![id],
                row_to_installed,
            )
            .optional()?;

        match result {
            Some(mut resource) => {
                resource.authors = self.get_authors_for(&resource.id)?;
                Ok(Some(resource))
            }
            None => Ok(None),
        }
    }

    /// List all installed resources, optionally filtered by game ID.
    pub fn list_installed(&self, game_id: Option<&str>) -> Result<Vec<InstalledResource>, DbError> {
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

        // Populate authors for each resource
        for resource in &mut resources {
            resource.authors = self.get_authors_for(&resource.id)?;
        }

        Ok(resources)
    }

    /// Remove an installed resource by ID.
    pub fn remove_installed(&self, id: &str) -> Result<bool, DbError> {
        // resource_authors cleaned up by ON DELETE CASCADE
        let count = self
            .conn
            .execute("DELETE FROM installed_resources WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    /// Count installed resources.
    pub fn count_installed(&self) -> Result<u64, DbError> {
        let count: u64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM installed_resources", [], |row| {
                    row.get(0)
                })?;
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

    // --- Authors ---

    /// Get authors for a specific resource.
    fn get_authors_for(&self, resource_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT a.name FROM authors a
             JOIN resource_authors ra ON ra.author_id = a.id
             WHERE ra.resource_id = ?1
             ORDER BY a.name",
        )?;
        let rows = stmt.query_map(params![resource_id], |row| row.get(0))?;
        let mut authors = Vec::new();
        for row in rows {
            authors.push(row?);
        }
        Ok(authors)
    }

    /// List all known authors.
    pub fn list_authors(&self) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM authors ORDER BY name")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut authors = Vec::new();
        for row in rows {
            authors.push(row?);
        }
        Ok(authors)
    }

    /// Find resources by author name (case-insensitive substring).
    pub fn find_by_author(&self, author: &str) -> Result<Vec<InstalledResource>, DbError> {
        let pattern = format!("%{author}%");
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT r.id, r.game_id, r.game_name, r.resource_type, r.version,
                    r.file_path, r.installed_at, r.vps_updated_at, r.metadata
             FROM installed_resources r
             JOIN resource_authors ra ON ra.resource_id = r.id
             JOIN authors a ON a.id = ra.author_id
             WHERE a.name LIKE ?1
             ORDER BY r.game_name, r.resource_type",
        )?;
        let rows = stmt.query_map(params![pattern], row_to_installed)?;
        let mut resources = Vec::new();
        for row in rows {
            let mut resource = row?;
            resource.authors = self.get_authors_for(&resource.id)?;
            resources.push(resource);
        }
        Ok(resources)
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
    pub fn download_history_for(&self, resource_id: &str) -> Result<Vec<DownloadRecord>, DbError> {
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
        authors: vec![], // populated separately
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
    pub authors: Vec<String>,
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
            file_path: format!("/path/to/{id}.vpx"),
            installed_at: None,
            vps_updated_at: Some(1745823539995),
            metadata: None,
            authors: vec![],
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
    fn upsert_deduplicates_by_file_path() {
        let db = test_db();

        // First import with one ID
        let mut res1 = sample_resource("old-id", "g1");
        res1.file_path = "/tables/hook.vpx".to_string();
        res1.version = Some("1.0".to_string());
        db.upsert_installed(&res1).unwrap();

        // Re-import same file with different ID (e.g., VPS resource ID found)
        let mut res2 = sample_resource("vps-resource-id", "g1");
        res2.file_path = "/tables/hook.vpx".to_string();
        res2.version = Some("2.0".to_string());
        db.upsert_installed(&res2).unwrap();

        // Should have one entry, not two
        assert_eq!(db.count_installed().unwrap(), 1);

        // Old ID should be gone
        assert!(db.get_installed("old-id").unwrap().is_none());

        // New ID should exist with updated metadata
        let fetched = db.get_installed("vps-resource-id").unwrap().unwrap();
        assert_eq!(fetched.version.as_deref(), Some("2.0"));
        assert_eq!(fetched.file_path, "/tables/hook.vpx");
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
        assert_eq!(history[0].file_path.as_deref(), Some("/path/to/file.zip"));
    }

    #[test]
    fn multiple_databases_independent() {
        let db1 = test_db();
        let db2 = test_db();

        db1.upsert_installed(&sample_resource("r1", "g1")).unwrap();

        assert_eq!(db1.count_installed().unwrap(), 1);
        assert_eq!(db2.count_installed().unwrap(), 0);
    }

    #[test]
    fn upsert_with_authors() {
        let db = test_db();
        let mut res = sample_resource("r1", "g1");
        res.authors = vec!["Author A".to_string(), "Author B".to_string()];
        db.upsert_installed(&res).unwrap();

        let fetched = db.get_installed("r1").unwrap().unwrap();
        assert_eq!(fetched.authors, vec!["Author A", "Author B"]);
    }

    #[test]
    fn upsert_replaces_authors() {
        let db = test_db();
        let mut res = sample_resource("r1", "g1");
        res.authors = vec!["Author A".to_string()];
        db.upsert_installed(&res).unwrap();

        res.authors = vec!["Author B".to_string(), "Author C".to_string()];
        db.upsert_installed(&res).unwrap();

        let fetched = db.get_installed("r1").unwrap().unwrap();
        assert_eq!(fetched.authors, vec!["Author B", "Author C"]);
    }

    #[test]
    fn authors_shared_across_resources() {
        let db = test_db();
        let mut r1 = sample_resource("r1", "g1");
        r1.authors = vec!["Shared Author".to_string(), "Unique A".to_string()];
        db.upsert_installed(&r1).unwrap();

        let mut r2 = sample_resource("r2", "g2");
        r2.authors = vec!["Shared Author".to_string(), "Unique B".to_string()];
        db.upsert_installed(&r2).unwrap();

        let all_authors = db.list_authors().unwrap();
        assert_eq!(all_authors, vec!["Shared Author", "Unique A", "Unique B"]);
    }

    #[test]
    fn find_by_author() {
        let db = test_db();
        let mut r1 = sample_resource("r1", "g1");
        r1.authors = vec!["JPSalas".to_string()];
        db.upsert_installed(&r1).unwrap();

        let mut r2 = sample_resource("r2", "g2");
        r2.authors = vec!["Other Author".to_string()];
        db.upsert_installed(&r2).unwrap();

        let results = db.find_by_author("jpsal").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "r1");
        assert_eq!(results[0].authors, vec!["JPSalas"]);
    }

    #[test]
    fn remove_cleans_up_author_associations() {
        let db = test_db();
        let mut res = sample_resource("r1", "g1");
        res.authors = vec!["Author A".to_string()];
        db.upsert_installed(&res).unwrap();

        db.remove_installed("r1").unwrap();

        // Author still exists in the authors table (shared resource)
        // but the association is gone
        let results = db.find_by_author("Author A").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn list_installed_includes_authors() {
        let db = test_db();
        let mut res = sample_resource("r1", "g1");
        res.authors = vec!["Author X".to_string()];
        db.upsert_installed(&res).unwrap();

        let all = db.list_installed(None).unwrap();
        assert_eq!(all[0].authors, vec!["Author X"]);
    }
}
