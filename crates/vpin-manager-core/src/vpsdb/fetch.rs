use std::fs;
use std::path::{Path, PathBuf};

use crate::vpsdb::models::Game;

const VPSDB_URL: &str =
    "https://virtualpinballspreadsheet.github.io/vps-db/db/vpsdb.json";
const LAST_UPDATED_URL: &str =
    "https://virtualpinballspreadsheet.github.io/vps-db/lastUpdated.json";
const DB_FILENAME: &str = "vpsdb.json";
const TIMESTAMP_FILENAME: &str = "vpsdb_last_updated";

/// Result of a sync operation.
#[derive(Debug)]
pub enum SyncResult {
    /// Database was downloaded (fresh or updated).
    Updated { game_count: usize },
    /// Local cache is already up to date.
    AlreadyCurrent { game_count: usize },
}

/// Manages fetching and caching the VPS database.
pub struct VpsDb {
    cache_dir: PathBuf,
    client: reqwest::Client,
}

impl VpsDb {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            client: reqwest::Client::new(),
        }
    }

    /// Path to the cached database file.
    fn db_path(&self) -> PathBuf {
        self.cache_dir.join(DB_FILENAME)
    }

    /// Path to the cached timestamp file.
    fn timestamp_path(&self) -> PathBuf {
        self.cache_dir.join(TIMESTAMP_FILENAME)
    }

    /// Read the locally cached timestamp, if any.
    fn read_local_timestamp(&self) -> Option<i64> {
        fs::read_to_string(self.timestamp_path())
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    /// Write the timestamp to the local cache.
    fn write_local_timestamp(&self, ts: i64) -> Result<(), std::io::Error> {
        fs::write(self.timestamp_path(), ts.to_string())
    }

    /// Fetch the remote lastUpdated timestamp.
    async fn fetch_remote_timestamp(&self) -> Result<i64, FetchError> {
        let text = self
            .client
            .get(LAST_UPDATED_URL)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        text.trim()
            .parse()
            .map_err(|_| FetchError::InvalidTimestamp(text))
    }

    /// Sync the database: check remote timestamp, download only if newer.
    /// Returns the parsed games and whether an update occurred.
    pub async fn sync(&self) -> Result<SyncResult, FetchError> {
        fs::create_dir_all(&self.cache_dir)?;

        let remote_ts = self.fetch_remote_timestamp().await?;
        let local_ts = self.read_local_timestamp();

        let needs_download = match local_ts {
            Some(local) if local >= remote_ts => false,
            _ => true,
        };

        if needs_download || !self.db_path().exists() {
            let bytes = self
                .client
                .get(VPSDB_URL)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            fs::write(self.db_path(), &bytes)?;
            self.write_local_timestamp(remote_ts)?;

            let games: Vec<Game> = serde_json::from_slice(&bytes)?;
            Ok(SyncResult::Updated {
                game_count: games.len(),
            })
        } else {
            let games = self.load_cached()?;
            Ok(SyncResult::AlreadyCurrent {
                game_count: games.len(),
            })
        }
    }

    /// Force a full download regardless of timestamps.
    pub async fn force_sync(&self) -> Result<SyncResult, FetchError> {
        fs::create_dir_all(&self.cache_dir)?;

        let remote_ts = self.fetch_remote_timestamp().await?;
        let bytes = self
            .client
            .get(VPSDB_URL)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        fs::write(self.db_path(), &bytes)?;
        self.write_local_timestamp(remote_ts)?;

        let games: Vec<Game> = serde_json::from_slice(&bytes)?;
        Ok(SyncResult::Updated {
            game_count: games.len(),
        })
    }

    /// Load games from the local cache. Returns an error if no cache exists.
    pub fn load_cached(&self) -> Result<Vec<Game>, FetchError> {
        let path = self.db_path();
        if !path.exists() {
            return Err(FetchError::NoCachedDb);
        }
        let bytes = fs::read(&path)?;
        let games: Vec<Game> = serde_json::from_slice(&bytes)?;
        Ok(games)
    }

    /// Check if a local cache exists.
    pub fn has_cache(&self) -> bool {
        self.db_path().exists()
    }

    /// Return the default cache directory using the `directories` crate.
    pub fn default_cache_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "vpin-manager")
            .map(|dirs| dirs.cache_dir().to_path_buf())
    }
}

#[derive(Debug)]
pub enum FetchError {
    Http(reqwest::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidTimestamp(String),
    NoCachedDb,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Http(e) => write!(f, "HTTP error: {e}"),
            FetchError::Io(e) => write!(f, "I/O error: {e}"),
            FetchError::Json(e) => write!(f, "JSON parse error: {e}"),
            FetchError::InvalidTimestamp(s) => {
                write!(f, "invalid timestamp from server: {s:?}")
            }
            FetchError::NoCachedDb => write!(
                f,
                "no cached database found, run 'vpin-manager sync' first"
            ),
        }
    }
}

impl std::error::Error for FetchError {}

impl From<reqwest::Error> for FetchError {
    fn from(e: reqwest::Error) -> Self {
        FetchError::Http(e)
    }
}

impl From<std::io::Error> for FetchError {
    fn from(e: std::io::Error) -> Self {
        FetchError::Io(e)
    }
}

impl From<serde_json::Error> for FetchError {
    fn from(e: serde_json::Error) -> Self {
        FetchError::Json(e)
    }
}

/// Convenience: resolve the cache dir, falling back to a provided override.
pub fn resolve_cache_dir(override_dir: Option<&Path>) -> PathBuf {
    match override_dir {
        Some(dir) => dir.to_path_buf(),
        None => VpsDb::default_cache_dir()
            .unwrap_or_else(|| PathBuf::from(".vpin-manager")),
    }
}
