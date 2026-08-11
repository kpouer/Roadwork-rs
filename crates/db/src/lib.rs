//! Persistence/caching for the WME extension.
//!
//! On native this is backed by SQLite (used for tests only). On wasm32 it is
//! backed by IndexedDB through `web-sys` — no C compiler required, and every
//! write is persisted by the browser automatically.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
use roadwork_core::model::opendata::Opendata;
use roadwork_core::model::opendata_data::OpendataData;
#[cfg(not(target_arch = "wasm32"))]
use roadwork_core::model::roadwork::Roadwork;
use roadwork_core::model::roadwork_data::RoadworkData;
#[cfg(not(target_arch = "wasm32"))]
use roadwork_core::model::wkt::polygon::Polygon;
use roadwork_core::now_millis;

#[cfg(not(target_arch = "wasm32"))]
use rusqlite::{Connection, OptionalExtension, params};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

#[cfg(target_arch = "wasm32")]
const ROADWORK_STORE: &str = "roadworks";
#[cfg(target_arch = "wasm32")]
const OPENDATA_STORE: &str = "opendata";
#[cfg(target_arch = "wasm32")]
const KV_STORE: &str = "kv";

#[derive(Debug)]
pub enum Error {
    #[cfg(not(target_arch = "wasm32"))]
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    #[cfg(target_arch = "wasm32")]
    Js(wasm_bindgen::JsValue),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Error::Sqlite(e) => write!(f, "sqlite: {e}"),
            Error::Json(e) => write!(f, "json: {e}"),
            #[cfg(target_arch = "wasm32")]
            Error::Js(e) => write!(f, "js: {e:?}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
impl From<wasm_bindgen::JsValue> for Error {
    fn from(e: wasm_bindgen::JsValue) -> Self {
        Error::Js(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Whether a snapshot fetched at `fetched_at_ms` is older than `max_age_ms`.
fn is_stale(fetched_at_ms: i64, max_age_ms: u64) -> bool {
    (now_millis() as i64).saturating_sub(fetched_at_ms) >= max_age_ms as i64
}

#[cfg(target_arch = "wasm32")]
pub struct Store {
    db: web_sys::IdbDatabase,
}

#[cfg(target_arch = "wasm32")]
impl Store {
    /// Opens the cache, connecting to the `roadwork` IndexedDB database.
    pub async fn open() -> Result<Self> {
        let db = indexed_db::open().await?;
        Ok(Self { db })
    }

    /// Replaces the cached roadworks for `service`, recording `now` as the
    /// fetched time.
    pub async fn save_roadworks(&mut self, service: &str, data: &RoadworkData) -> Result<()> {
        let payload = serde_json::to_vec(data)?;
        let value = indexed_db::record(Some(now_millis() as i64), &payload);
        indexed_db::put(&self.db, ROADWORK_STORE, service, &value).await?;
        Ok(())
    }

    /// Returns the cached roadworks for `service`, if present and not older
    /// than `max_age_ms`.
    pub async fn get_roadworks_cached(
        &self,
        service: &str,
        max_age_ms: u64,
    ) -> Result<Option<RoadworkData>> {
        let Some((fetched_at, payload)) = self.read_roadworks(service).await? else {
            return Ok(None);
        };
        let Some(fetched_at) = fetched_at else {
            return Ok(None);
        };
        if is_stale(fetched_at, max_age_ms) {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&payload)?))
    }

    /// Returns the cached roadworks for `service`, regardless of age.
    pub async fn get_roadworks(&self, service: &str) -> Result<Option<RoadworkData>> {
        let Some((_, payload)) = self.read_roadworks(service).await? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(&payload)?))
    }

    async fn read_roadworks(&self, service: &str) -> Result<Option<(Option<i64>, Vec<u8>)>> {
        match indexed_db::get(&self.db, ROADWORK_STORE, service).await? {
            Some(value) => Ok(Some(indexed_db::decode_record(&value)?)),
            None => Ok(None),
        }
    }

    /// Replaces the cached opendata items for `service`. Opendata never expires.
    pub async fn save_opendata(&mut self, service: &str, data: &OpendataData) -> Result<()> {
        let payload = serde_json::to_vec(data)?;
        let value = indexed_db::record(None, &payload);
        indexed_db::put(&self.db, OPENDATA_STORE, service, &value).await?;
        Ok(())
    }

    /// Returns the cached opendata items for `service`.
    pub async fn get_opendata(&self, service: &str) -> Result<Option<OpendataData>> {
        let Some(value) = indexed_db::get(&self.db, OPENDATA_STORE, service).await? else {
            return Ok(None);
        };
        let (_, payload) = indexed_db::decode_record(&value)?;
        Ok(Some(serde_json::from_slice(&payload)?))
    }

    /// Removes every cached row for `service` (roadworks and opendata).
    pub async fn remove(&mut self, service: &str) -> Result<()> {
        indexed_db::delete(&self.db, ROADWORK_STORE, service).await?;
        indexed_db::delete(&self.db, OPENDATA_STORE, service).await?;
        Ok(())
    }

    /// Removes the cached roadworks for `service`.
    pub async fn remove_roadworks(&mut self, service: &str) -> Result<()> {
        indexed_db::delete(&self.db, ROADWORK_STORE, service).await?;
        Ok(())
    }

    /// Removes the cached opendata for `service`.
    pub async fn remove_opendata(&mut self, service: &str) -> Result<()> {
        indexed_db::delete(&self.db, OPENDATA_STORE, service).await?;
        Ok(())
    }

    /// Stores an arbitrary string under `key` (miscellaneous state such as
    /// polygon groups).
    pub async fn put_raw(&mut self, key: &str, value: &str) -> Result<()> {
        indexed_db::put(&self.db, KV_STORE, key, &JsValue::from_str(value)).await?;
        Ok(())
    }

    /// Returns the string stored under `key`, if any.
    pub async fn get_raw(&self, key: &str) -> Result<Option<String>> {
        match indexed_db::get(&self.db, KV_STORE, key).await? {
            Some(value) => Ok(value.as_string()),
            None => Ok(None),
        }
    }

    /// Removes every cached row.
    pub async fn clear_all(&mut self) -> Result<()> {
        indexed_db::clear(&self.db, ROADWORK_STORE).await?;
        indexed_db::clear(&self.db, OPENDATA_STORE).await?;
        indexed_db::clear(&self.db, KV_STORE).await?;
        Ok(())
    }

    /// IndexedDB writes are persisted automatically; kept for API parity.
    pub async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Store {
    conn: Connection,
}

#[cfg(not(target_arch = "wasm32"))]
impl Store {
    /// Opens the store on a real file (native builds / tests).
    pub fn open() -> Result<Self> {
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
        upsert_fetched_at(&tx, service, fetched_at)?;
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
                "SELECT fetched_at FROM cache WHERE service = ?1",
                params![service],
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
        upsert_fetched_at(&tx, service, fetched_at)?;
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
        tx.execute("DELETE FROM cache WHERE service = ?1", params![service])?;
        tx.commit()?;
        Ok(())
    }

    /// Removes every cached row.
    pub fn clear_all(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM data", [])?;
        tx.execute("DELETE FROM roadwork", [])?;
        tx.execute("DELETE FROM cache", [])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn upsert_fetched_at(tx: &rusqlite::Transaction<'_>, service: &str, fetched_at: i64) -> Result<()> {
    tx.execute(
        "INSERT INTO cache(service, fetched_at) VALUES (?1, ?2)
         ON CONFLICT(service) DO UPDATE SET fetched_at = excluded.fetched_at",
        params![service, fetched_at],
    )?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
const DDL: &str = "
PRAGMA user_version = 1;
CREATE TABLE IF NOT EXISTS cache (
    service    TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (service)
);
CREATE TABLE IF NOT EXISTS data (
    service     TEXT NOT NULL,
    id          TEXT NOT NULL,
    latitude    REAL NOT NULL,
    longitude   REAL NOT NULL,
    polygons    TEXT,
    description TEXT,
    PRIMARY KEY (service, id),
    FOREIGN KEY (service) REFERENCES cache(service) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_data_location ON data(latitude, longitude);
CREATE TABLE IF NOT EXISTS roadwork (
    service   TEXT NOT NULL,
    id        TEXT NOT NULL,
    latitude  REAL NOT NULL,
    longitude REAL NOT NULL,
    payload   BLOB NOT NULL,
    PRIMARY KEY (service, id),
    FOREIGN KEY (service) REFERENCES cache(service) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_roadwork_location ON roadwork(latitude, longitude);
";

#[cfg(not(target_arch = "wasm32"))]
fn init_schema(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(&format!("PRAGMA foreign_keys = ON;\n{DDL}"))?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
mod indexed_db;

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
}
