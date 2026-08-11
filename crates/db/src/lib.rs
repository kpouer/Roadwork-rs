//! Persistence/caching for the WME extension, backed by SQLite.
//!
//! On native this is backed by bundled SQLite (used for tests only). On wasm32
//! the same `rusqlite` code runs against `sqlite-wasm-rs`, persisting to the
//! Origin Private File System (OPFS) through the `sahpool` VFS
//! (`sqlite-wasm-vfs`). OPFS requires a dedicated worker at runtime, so the
//! wasm module runs inside the extension's `wasm-worker.js`.

use std::collections::HashMap;
use std::path::Path;

use roadwork_core::model::opendata::Opendata;
use roadwork_core::model::opendata_data::OpendataData;
use roadwork_core::model::roadwork::Roadwork;
use roadwork_core::model::roadwork_data::RoadworkData;
use roadwork_core::model::wkt::polygon::Polygon;
use roadwork_core::now_millis;

use rusqlite::{Connection, OptionalExtension, params};

/// What kind of snapshot a `cache` row describes.
#[derive(Debug, Clone, Copy)]
pub enum CacheType {
    Roadwork,
    Opendata,
}

impl CacheType {
    pub fn as_str(self) -> &'static str {
        match self {
            CacheType::Roadwork => "roadwork",
            CacheType::Opendata => "opendata",
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Vfs(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Sqlite(e) => write!(f, "sqlite: {e}"),
            Error::Json(e) => write!(f, "json: {e}"),
            Error::Vfs(e) => write!(f, "vfs: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Sqlite(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Whether a snapshot fetched at `fetched_at_ms` is older than `max_age_ms`.
fn is_stale(fetched_at_ms: i64, max_age_ms: u64) -> bool {
    (now_millis() as i64).saturating_sub(fetched_at_ms) >= max_age_ms as i64
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens the store. On wasm32 this installs the OPFS `sahpool` VFS first
    /// (a dedicated worker is required at runtime); on native it opens a file.
    pub async fn open() -> Result<Self> {
        #[cfg(target_arch = "wasm32")]
        {
            sqlite_wasm_vfs::sahpool::install::<rusqlite::ffi::WasmOsCallback>(
                &sqlite_wasm_vfs::sahpool::OpfsSAHPoolCfg::default(),
                true,
            )
            .await
            .map_err(|e| Error::Vfs(e.to_string()))?;
        }
        Self::open_path("roadwork.db")
    }

    /// Opens the store on a real file (native builds / tests).
    pub fn open_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        init_schema(&mut conn)?;
        Ok(Self { conn })
    }

    /// Replaces the cached roadworks for `service`, bumping `fetched_at`.
    pub fn save_roadworks(&mut self, service: &str, data: &RoadworkData) -> Result<()> {
        let fetched_at = now_millis() as i64;
        let tx = self.conn.transaction()?;
        upsert_fetched_at(&tx, service, CacheType::Roadwork, fetched_at)?;
        tx.execute("DELETE FROM roadwork WHERE service = ?1", params![service])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO roadwork(service, id, latitude, longitude, payload) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for rw in data.iter() {
                let payload = serde_json::to_vec(rw)?;
                stmt.execute(params![
                    service,
                    rw.opendata.id,
                    rw.opendata.latitude,
                    rw.opendata.longitude,
                    payload
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Returns the cached roadworks for `service`, if present and not older
    /// than `max_age_ms`.
    pub fn get_roadworks_cached(
        &self,
        service: &str,
        max_age_ms: u64,
    ) -> Result<Option<RoadworkData>> {
        let fetched_at: Option<i64> = self
            .conn
            .query_row(
                "SELECT fetched_at FROM cache WHERE service = ?1 AND type = ?2",
                params![service, CacheType::Roadwork.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(fetched_at) = fetched_at else {
            return Ok(None);
        };
        if is_stale(fetched_at, max_age_ms) {
            return Ok(None);
        }
        self.get_roadworks(service)
    }

    /// Returns the cached roadworks for `service`, regardless of age.
    pub fn get_roadworks(&self, service: &str) -> Result<Option<RoadworkData>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, payload FROM roadwork WHERE service = ?1")?;
        let rows = stmt.query_map(params![service], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut roadworks = HashMap::new();
        for row in rows {
            let (id, payload) = row?;
            let rw: Roadwork = serde_json::from_slice(&payload)?;
            roadworks.insert(id, rw);
        }
        if roadworks.is_empty() {
            return Ok(None);
        }
        Ok(Some(RoadworkData {
            source: service.to_string(),
            roadworks,
            created: now_millis(),
        }))
    }

    /// Replaces the cached opendata items for `service`, bumping `fetched_at`.
    pub fn save_opendata(&mut self, service: &str, data: &OpendataData) -> Result<()> {
        let fetched_at = now_millis() as i64;
        let tx = self.conn.transaction()?;
        upsert_fetched_at(&tx, service, CacheType::Opendata, fetched_at)?;
        tx.execute("DELETE FROM data WHERE service = ?1", params![service])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO data(service, id, latitude, longitude, polygons, description) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for item in data.iter() {
                let polygons = item
                    .polygons
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?;
                stmt.execute(params![
                    service,
                    item.id,
                    item.latitude,
                    item.longitude,
                    polygons,
                    item.description
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Returns the cached opendata items for `service`. Opendata never expires.
    pub fn get_opendata(&self, service: &str) -> Result<Option<OpendataData>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, latitude, longitude, polygons, description FROM data WHERE service = ?1",
        )?;
        let rows = stmt.query_map(params![service], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut opendata = HashMap::new();
        for row in rows {
            let (id, latitude, longitude, polygons, description) = row?;
            let polygons: Option<Vec<Polygon>> =
                polygons.map(|p| serde_json::from_str(&p)).transpose()?;
            opendata.insert(
                id.clone(),
                Opendata {
                    id,
                    latitude,
                    longitude,
                    polygons,
                    description,
                },
            );
        }
        if opendata.is_empty() {
            return Ok(None);
        }
        Ok(Some(OpendataData {
            source: service.to_string(),
            opendata,
            created: now_millis(),
        }))
    }

    /// Removes every cached row for `service` (roadworks, opendata, cache).
    pub fn remove(&mut self, service: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM roadwork WHERE service = ?1", params![service])?;
        tx.execute("DELETE FROM data WHERE service = ?1", params![service])?;
        tx.execute("DELETE FROM cache WHERE service = ?1", params![service])?;
        tx.commit()?;
        Ok(())
    }

    /// Removes the cached roadworks for `service`.
    pub fn remove_roadworks(&mut self, service: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM roadwork WHERE service = ?1", params![service])?;
        tx.execute(
            "DELETE FROM cache WHERE service = ?1 AND type = ?2",
            params![service, CacheType::Roadwork.as_str()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Removes the cached opendata for `service`.
    pub fn remove_opendata(&mut self, service: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM data WHERE service = ?1", params![service])?;
        tx.execute(
            "DELETE FROM cache WHERE service = ?1 AND type = ?2",
            params![service, CacheType::Opendata.as_str()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Stores an arbitrary string under `key` (miscellaneous state such as
    /// polygon groups).
    pub fn put_raw(&mut self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Returns the string stored under `key`, if any.
    pub fn get_raw(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM kv WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Removes every cached row.
    pub fn clear_all(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM data", [])?;
        tx.execute("DELETE FROM roadwork", [])?;
        tx.execute("DELETE FROM cache", [])?;
        tx.execute("DELETE FROM kv", [])?;
        tx.commit()?;
        Ok(())
    }
}

fn upsert_fetched_at(
    tx: &rusqlite::Transaction<'_>,
    service: &str,
    cache_type: CacheType,
    fetched_at: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO cache(service, type, fetched_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(service, type) DO UPDATE SET fetched_at = excluded.fetched_at",
        params![service, cache_type.as_str(), fetched_at],
    )?;
    Ok(())
}

const DDL: &str = "
PRAGMA user_version = 2;
CREATE TABLE IF NOT EXISTS cache (
    service    TEXT NOT NULL,
    type       TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (service, type)
);
CREATE TABLE IF NOT EXISTS data (
    service     TEXT NOT NULL,
    id          TEXT NOT NULL,
    latitude    REAL NOT NULL,
    longitude   REAL NOT NULL,
    polygons    TEXT,
    description TEXT,
    PRIMARY KEY (service, id)
);
CREATE INDEX IF NOT EXISTS idx_data_location ON data(latitude, longitude);
CREATE TABLE IF NOT EXISTS roadwork (
    service   TEXT NOT NULL,
    id        TEXT NOT NULL,
    latitude  REAL NOT NULL,
    longitude REAL NOT NULL,
    payload   BLOB NOT NULL,
    PRIMARY KEY (service, id)
);
CREATE INDEX IF NOT EXISTS idx_roadwork_location ON roadwork(latitude, longitude);
CREATE TABLE IF NOT EXISTS kv (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

fn init_schema(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(&format!("PRAGMA foreign_keys = ON;\n{DDL}"))?;
    Ok(())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn sample_roadworks(service: &str) -> RoadworkData {
        let mut roadworks = HashMap::new();
        roadworks.insert(
            "id-1".to_string(),
            Roadwork {
                opendata: Opendata {
                    id: "id-1".to_string(),
                    latitude: 48.85,
                    longitude: 2.35,
                    polygons: None,
                    description: Some("Work".to_string()),
                },
                start: 0,
                end: 0,
                road: None,
                location_details: None,
                impact_circulation_detail: None,
                sync_data: Default::default(),
                url: String::new(),
            },
        );
        RoadworkData {
            source: service.to_string(),
            roadworks,
            created: now_millis(),
        }
    }

    fn sample_opendata(service: &str) -> OpendataData {
        let mut items = HashMap::new();
        items.insert(
            "od-1".to_string(),
            Opendata {
                id: "od-1".to_string(),
                latitude: 48.8,
                longitude: 2.3,
                polygons: Some(vec![Polygon {
                    xpoints: vec![2.2, 2.3],
                    ypoints: vec![48.7, 48.8],
                }]),
                description: Some("Item".to_string()),
            },
        );
        OpendataData {
            source: service.to_string(),
            opendata: items,
            created: now_millis(),
        }
    }

    #[test]
    fn roundtrips_roadworks_and_opendata() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "roadwork-db-test-roundtrips-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        {
            let mut store = Store::open_path(&path).unwrap();
            store
                .save_roadworks("France-Paris", &sample_roadworks("France-Paris"))
                .unwrap();
            store
                .save_opendata("France-Paris", &sample_opendata("France-Paris"))
                .unwrap();

            let roadworks = store.get_roadworks("France-Paris").unwrap().unwrap();
            assert_eq!(roadworks.roadworks.len(), 1);
            assert_eq!(
                roadworks.roadworks["id-1"].opendata.description.as_deref(),
                Some("Work")
            );

            let cached = store
                .get_roadworks_cached("France-Paris", 86_400_000)
                .unwrap();
            assert!(cached.is_some());

            let opendata = store.get_opendata("France-Paris").unwrap().unwrap();
            assert_eq!(opendata.opendata.len(), 1);
            assert_eq!(
                opendata.opendata["od-1"].polygons.as_ref().unwrap()[0].xpoints,
                vec![2.2, 2.3]
            );
        }
        // Re-open from disk: persistence across connections.
        {
            let store = Store::open_path(&path).unwrap();
            assert!(store.get_roadworks("France-Paris").unwrap().is_some());
            assert!(store.get_opendata("France-Paris").unwrap().is_some());
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn expiry_and_removal() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("roadwork-db-test-exp-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut store = Store::open_path(&path).unwrap();
        store.save_roadworks("A", &sample_roadworks("A")).unwrap();
        // TTL of 0 → always stale.
        assert!(store.get_roadworks_cached("A", 0).unwrap().is_none());
        // Uncached service → none.
        assert!(store.get_roadworks("Z").unwrap().is_none());
        store.remove("A").unwrap();
        assert!(store.get_roadworks("A").unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn typed_removal_and_kv() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("roadwork-db-test-kv-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut store = Store::open_path(&path).unwrap();
        store.save_roadworks("A", &sample_roadworks("A")).unwrap();
        store.save_opendata("A", &sample_opendata("A")).unwrap();

        // Removing roadworks leaves opendata intact.
        store.remove_roadworks("A").unwrap();
        assert!(store.get_roadworks("A").unwrap().is_none());
        assert!(store.get_opendata("A").unwrap().is_some());

        // kv roundtrip.
        assert!(store.get_raw("k").unwrap().is_none());
        store.put_raw("k", "v").unwrap();
        assert_eq!(store.get_raw("k").unwrap().as_deref(), Some("v"));
        store.put_raw("k", "v2").unwrap();
        assert_eq!(store.get_raw("k").unwrap().as_deref(), Some("v2"));

        // clear_all wipes everything including kv.
        store.clear_all().unwrap();
        assert!(store.get_opendata("A").unwrap().is_none());
        assert!(store.get_raw("k").unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }
}
