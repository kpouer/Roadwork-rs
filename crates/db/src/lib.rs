//! Persistence/caching for the WME extension, backed by SQLite.
//!
//! The `rusqlite` code runs against `sqlite-wasm-rs`, persisting to the
//! Origin Private File System (OPFS) through the `sahpool` VFS
//! (`sqlite-wasm-vfs`). OPFS requires a dedicated worker at runtime, so the
//! wasm module runs inside the extension's `wasm-worker.js`.

use std::collections::HashMap;

use roadwork_core::model::opendata::Opendata;
use roadwork_core::model::opendata_data::OpendataData;
use roadwork_core::model::roadwork::Roadwork;
use roadwork_core::model::roadwork_data::RoadworkData;
use roadwork_core::model::wkt::polygon::Polygon;
use roadwork_core::now_millis;

use rusqlite::{Connection, OptionalExtension, params};

/// Describes a table column for the DB explorer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub primary_key: bool,
}

/// A single cell value in a DB explorer row.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum Cell {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Cell {
    /// Renders the cell for display in the UI.
    pub fn display(&self) -> String {
        match self {
            Cell::Null => String::new(),
            Cell::Integer(i) => i.to_string(),
            Cell::Real(f) => {
                if f.fract() == 0.0 {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            Cell::Text(s) => s.clone(),
            Cell::Blob(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        }
    }
}

/// The result of a paginated table read for the DB explorer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DbTableData {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<Cell>>,
    pub total: i64,
}

/// A table and its row count, for the DB explorer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableInfo {
    pub name: String,
    pub count: i64,
}

/// A per-service element count, for the DB explorer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceCount {
    pub service: String,
    pub count: i64,
}

/// A summary of the whole database for the DB explorer: table row counts, the
/// total size, and per-service counts for roadworks and opendata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DbOverview {
    pub tables: Vec<TableInfo>,
    pub size_bytes: i64,
    pub roadwork_by_service: Vec<ServiceCount>,
    pub opendata_by_service: Vec<ServiceCount>,
}

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
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Sqlite(e) => write!(f, "sqlite: {e}"),
            Error::Json(e) => write!(f, "json: {e}"),
            Error::Vfs(e) => write!(f, "vfs: {e}"),
            Error::Other(e) => write!(f, "{e}"),
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
    /// Opens the store, installing the OPFS `sahpool` VFS first (a dedicated
    /// worker is required at runtime). The VFS install is gated on wasm32 only
    /// because the `WasmOsCallback` FFI type does not exist on other targets.
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
        let mut conn = Connection::open("roadwork.db")?;
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
        collect_roadworks(service, rows)
    }

    /// Returns the cached roadworks for `service` whose point falls inside the
    /// given latitude/longitude rectangle. The bound order does not matter.
    pub fn get_roadworks_in_bbox(
        &self,
        service: &str,
        lat_min: f64,
        lon_min: f64,
        lat_max: f64,
        lon_max: f64,
    ) -> Result<Option<RoadworkData>> {
        let (lat_min, lat_max) = ordered(lat_min, lat_max);
        let (lon_min, lon_max) = ordered(lon_min, lon_max);
        let mut stmt = self.conn.prepare(
            "SELECT id, payload FROM roadwork
             WHERE service = ?1 AND latitude BETWEEN ?2 AND ?3 AND longitude BETWEEN ?4 AND ?5",
        )?;
        let rows = stmt.query_map(
            params![service, lat_min, lat_max, lon_min, lon_max],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )?;
        collect_roadworks(service, rows)
    }

    /// Replaces the cached opendata items for `service`, bumping `fetched_at`.
    pub fn save_opendata(&mut self, service: &str, data: &OpendataData) -> Result<()> {
        let fetched_at = now_millis() as i64;
        let tx = self.conn.transaction()?;
        upsert_fetched_at(&tx, service, CacheType::Opendata, fetched_at)?;
        tx.execute("DELETE FROM data WHERE service = ?1", params![service])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO data(service, id, latitude, longitude, polygons, description, reference) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
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
                    item.description,
                    item.reference
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Returns the cached opendata items for `service`. Opendata never expires.
    pub fn get_opendata(&self, service: &str) -> Result<Option<OpendataData>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, latitude, longitude, polygons, description, reference FROM data WHERE service = ?1",
        )?;
        let rows = stmt.query_map(params![service], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        collect_opendata(service, rows)
    }

    /// Returns the cached opendata items for `service` whose point falls inside
    /// the given latitude/longitude rectangle. The bound order does not matter.
    /// When `limit` is provided, at most that many items are returned, ordered
    /// by `id` for a deterministic subset.
    pub fn get_opendata_in_bbox(
        &self,
        service: &str,
        lat_min: f64,
        lon_min: f64,
        lat_max: f64,
        lon_max: f64,
        limit: Option<u64>,
    ) -> Result<Option<OpendataData>> {
        let (lat_min, lat_max) = ordered(lat_min, lat_max);
        let (lon_min, lon_max) = ordered(lon_min, lon_max);
        let mut sql = String::from(
            "SELECT id, latitude, longitude, polygons, description, reference FROM data
             WHERE service = ?1 AND latitude BETWEEN ?2 AND ?3 AND longitude BETWEEN ?4 AND ?5",
        );
        if limit.is_some() {
            sql.push_str(" ORDER BY id LIMIT ?6");
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let map = |row: &rusqlite::Row<'_>| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        };
        let rows = match limit {
            Some(limit) => stmt.query_map(
                params![service, lat_min, lat_max, lon_min, lon_max, limit as i64],
                map,
            )?,
            None => stmt.query_map(params![service, lat_min, lat_max, lon_min, lon_max], map)?,
        };
        collect_opendata(service, rows)
    }

    /// Returns the number of cached opendata items per service.
    pub fn opendata_counts(&self) -> Result<HashMap<String, i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT service, COUNT(*) FROM data GROUP BY service")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut counts = HashMap::new();
        for row in rows {
            let (service, count) = row?;
            counts.insert(service, count);
        }
        Ok(counts)
    }

    /// Returns the number of cached roadworks per service, sorted by service.
    /// Every service with a `cache` snapshot appears, even with zero items.
    pub fn roadwork_counts(&self) -> Result<Vec<ServiceCount>> {
        self.service_counts(
            "SELECT c.service, COUNT(r.service)
             FROM cache c
             LEFT JOIN roadwork r ON r.service = c.service
             WHERE c.type = 'roadwork'
             GROUP BY c.service",
        )
    }

    /// Returns the number of cached opendata items per service, sorted by
    /// service. Every service with a `cache` snapshot appears, even with zero
    /// items.
    pub fn opendata_counts_list(&self) -> Result<Vec<ServiceCount>> {
        self.service_counts(
            "SELECT c.service, COUNT(d.service)
             FROM cache c
             LEFT JOIN data d ON d.service = c.service
             WHERE c.type = 'opendata'
             GROUP BY c.service",
        )
    }

    /// Runs a `GROUP BY service` count query, sorting the result by service.
    fn service_counts(&self, sql: &str) -> Result<Vec<ServiceCount>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(ServiceCount {
                service: row.get::<_, String>(0)?,
                count: row.get::<_, i64>(1)?,
            })
        })?;
        let mut counts: Vec<ServiceCount> = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)?;
        counts.sort_by(|a, b| a.service.cmp(&b.service));
        Ok(counts)
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
            .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |row| {
                row.get(0)
            })
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

    /// Lists the application tables (excluding the `sqlite_*` internals).
    pub fn list_tables(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// Returns a summary of the whole database for the DB explorer: table row
    /// counts, total size, and per-service counts for roadworks and opendata.
    pub fn overview(&self) -> Result<DbOverview> {
        let tables = self
            .list_tables()?
            .into_iter()
            .map(|name| {
                let count = self.table_count(&name)?;
                Ok(TableInfo { name, count })
            })
            .collect::<Result<Vec<_>>>()?;
        let page_count: i64 = self
            .conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let page_size: i64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))?;
        Ok(DbOverview {
            tables,
            size_bytes: page_count.saturating_mul(page_size),
            roadwork_by_service: self.roadwork_counts()?,
            opendata_by_service: self.opendata_counts_list()?,
        })
    }

    /// Returns the columns of `table`, in definition order.
    pub fn table_columns(&self, table: &str) -> Result<Vec<ColumnInfo>> {
        self.ensure_table(table)?;
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
        let rows = stmt.query_map([], |row| {
            Ok(ColumnInfo {
                name: row.get::<_, String>(1)?,
                primary_key: row.get::<_, i64>(5)? > 0,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)
    }

    /// Returns the number of rows in `table`.
    pub fn table_count(&self, table: &str) -> Result<i64> {
        self.ensure_table(table)?;
        self.conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |row| {
                row.get(0)
            })
            .map_err(Error::from)
    }

    /// Reads a page of `table` ordered by `rowid`, returning columns, rows and
    /// the total number of rows. When `bbox` is given and `table` has
    /// `latitude`/`longitude` columns, only the rows inside that rectangle are
    /// returned (and counted); the bound order does not matter.
    pub fn table_rows(
        &self,
        table: &str,
        offset: i64,
        limit: i64,
        bbox: Option<(f64, f64, f64, f64)>,
    ) -> Result<DbTableData> {
        self.ensure_table(table)?;
        let columns = self.table_columns(table)?;
        let has_location = columns.iter().any(|c| c.name == "latitude")
            && columns.iter().any(|c| c.name == "longitude");
        let bbox = bbox
            .filter(|_| has_location)
            .map(|(lat_min, lon_min, lat_max, lon_max)| {
                let (lat_min, lat_max) = ordered(lat_min, lat_max);
                let (lon_min, lon_max) = ordered(lon_min, lon_max);
                (lat_min, lon_min, lat_max, lon_max)
            });
        let total = self.table_count_filtered(table, bbox)?;
        let (select_sql, page_params): (String, Vec<rusqlite::types::Value>) = match bbox {
            Some((lat_min, lon_min, lat_max, lon_max)) => (
                format!(
                    "SELECT * FROM \"{table}\"
                     WHERE latitude BETWEEN ?1 AND ?2 AND longitude BETWEEN ?3 AND ?4
                     ORDER BY rowid LIMIT ?5 OFFSET ?6"
                ),
                vec![
                    rusqlite::types::Value::Real(lat_min),
                    rusqlite::types::Value::Real(lat_max),
                    rusqlite::types::Value::Real(lon_min),
                    rusqlite::types::Value::Real(lon_max),
                    rusqlite::types::Value::Integer(limit),
                    rusqlite::types::Value::Integer(offset),
                ],
            ),
            None => (
                format!("SELECT * FROM \"{table}\" ORDER BY rowid LIMIT ?1 OFFSET ?2"),
                vec![
                    rusqlite::types::Value::Integer(limit),
                    rusqlite::types::Value::Integer(offset),
                ],
            ),
        };
        let mut stmt = self.conn.prepare(&select_sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(page_params), |row| {
                let mut cells = Vec::with_capacity(columns.len());
                for i in 0..columns.len() {
                    let value: rusqlite::types::Value = row.get(i)?;
                    cells.push(cell_from_value(value));
                }
                Ok(cells)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)?;
        Ok(DbTableData {
            columns,
            rows,
            total,
        })
    }

    /// Returns the number of rows of `table`, optionally filtered by a
    /// `latitude`/`longitude` rectangle.
    fn table_count_filtered(&self, table: &str, bbox: Option<(f64, f64, f64, f64)>) -> Result<i64> {
        let (sql, params): (String, Vec<rusqlite::types::Value>) = match bbox {
            Some((lat_min, lon_min, lat_max, lon_max)) => (
                format!(
                    "SELECT COUNT(*) FROM \"{table}\"
                     WHERE latitude BETWEEN ?1 AND ?2 AND longitude BETWEEN ?3 AND ?4"
                ),
                vec![
                    rusqlite::types::Value::Real(lat_min),
                    rusqlite::types::Value::Real(lat_max),
                    rusqlite::types::Value::Real(lon_min),
                    rusqlite::types::Value::Real(lon_max),
                ],
            ),
            None => (format!("SELECT COUNT(*) FROM \"{table}\""), Vec::new()),
        };
        self.conn
            .query_row(&sql, rusqlite::params_from_iter(params), |row| row.get(0))
            .map_err(Error::from)
    }

    /// Deletes the row of `table` identified by `keys` (column/value pairs).
    ///
    /// Deleting a row of the `cache` table also removes the linked cached rows
    /// for the same service (`roadwork` when type is `roadwork`, `data` when
    /// type is `opendata`), in the same transaction. Returns the number of
    /// deleted `table` rows.
    pub fn delete_table_row(
        &mut self,
        table: &str,
        keys: &[(String, serde_json::Value)],
    ) -> Result<usize> {
        self.ensure_table(table)?;
        if keys.is_empty() {
            return Err(Error::Other("No key given for row deletion".to_string()));
        }
        let tx = self.conn.transaction()?;
        let where_clause = keys
            .iter()
            .map(|(column, _)| format!("\"{column}\" = ?"))
            .collect::<Vec<_>>()
            .join(" AND ");
        let sql = format!("DELETE FROM \"{table}\" WHERE {where_clause}");
        let values: Vec<rusqlite::types::Value> = keys
            .iter()
            .map(|(_, value)| value_from_json(value))
            .collect();
        let deleted = tx.execute(&sql, rusqlite::params_from_iter(values))?;
        if table == "cache" && deleted > 0 {
            delete_linked_cache_rows(&tx, keys)?;
        }
        tx.commit()?;
        Ok(deleted)
    }

    /// Ensures `table` is one of the application tables, guarding the generic
    /// SQL builders against arbitrary identifiers.
    fn ensure_table(&self, table: &str) -> Result<()> {
        let tables = self.list_tables()?;
        if tables.iter().any(|t| t == table) {
            Ok(())
        } else {
            Err(Error::Other(format!("Unknown table: {table}")))
        }
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

/// Returns `(min, max)` for a pair of bounds, ordering them.
fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Converts a rusqlite value into an explorer [`Cell`].
fn cell_from_value(value: rusqlite::types::Value) -> Cell {
    match value {
        rusqlite::types::Value::Null => Cell::Null,
        rusqlite::types::Value::Integer(i) => Cell::Integer(i),
        rusqlite::types::Value::Real(f) => Cell::Real(f),
        rusqlite::types::Value::Text(s) => Cell::Text(s),
        rusqlite::types::Value::Blob(bytes) => Cell::Blob(bytes),
    }
}

/// Converts a JSON key value into a bindable rusqlite value.
fn value_from_json(value: &serde_json::Value) -> rusqlite::types::Value {
    match value {
        serde_json::Value::Null => rusqlite::types::Value::Null,
        serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        serde_json::Value::Bool(b) => rusqlite::types::Value::Integer(*b as i64),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(rusqlite::types::Value::Integer)
            .or_else(|| {
                n.as_u64()
                    .map(|u| rusqlite::types::Value::Integer(u as i64))
            })
            .or_else(|| n.as_f64().map(rusqlite::types::Value::Real))
            .unwrap_or(rusqlite::types::Value::Null),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => rusqlite::types::Value::Null,
    }
}

/// Removes the cached rows linked to a deleted `cache` row, given its keys.
fn delete_linked_cache_rows(
    tx: &rusqlite::Transaction<'_>,
    keys: &[(String, serde_json::Value)],
) -> Result<()> {
    let service = keys
        .iter()
        .find(|(column, _)| column == "service")
        .and_then(|(_, value)| value.as_str())
        .ok_or_else(|| Error::Other("cache row has no service key".to_string()))?;
    let cache_type = keys
        .iter()
        .find(|(column, _)| column == "type")
        .and_then(|(_, value)| value.as_str())
        .ok_or_else(|| Error::Other("cache row has no type key".to_string()))?;
    match cache_type {
        "roadwork" => {
            tx.execute("DELETE FROM roadwork WHERE service = ?1", params![service])?;
        }
        "opendata" => {
            tx.execute("DELETE FROM data WHERE service = ?1", params![service])?;
        }
        _ => {}
    }
    Ok(())
}

/// Assembles the roadwork rows into a `RoadworkData`, or `None` when empty.
fn collect_roadworks(
    service: &str,
    rows: impl Iterator<Item = rusqlite::Result<(String, Vec<u8>)>>,
) -> Result<Option<RoadworkData>> {
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

/// Assembles the opendata rows into an `OpendataData`, or `None` when empty.
fn collect_opendata(
    service: &str,
    rows: impl Iterator<
        Item = rusqlite::Result<(
            String,
            f64,
            f64,
            Option<String>,
            Option<String>,
            Option<String>,
        )>,
    >,
) -> Result<Option<OpendataData>> {
    let mut opendata = HashMap::new();
    for row in rows {
        let (id, latitude, longitude, polygons, description, reference) = row?;
        let polygons: Option<Vec<Polygon>> =
            polygons.map(|p| serde_json::from_str(&p)).transpose()?;
        opendata.insert(
            id.clone(),
            Opendata {
                id,
                reference,
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

const DDL: &str = "
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
    reference   TEXT,
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

const SCHEMA_VERSION: i64 = 3;

fn init_schema(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(&format!("PRAGMA foreign_keys = ON;\n{DDL}"))?;
    migrate(conn)?;
    Ok(())
}

/// Migrates an existing database to the current [`SCHEMA_VERSION`].
fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }
    if version < 3 {
        let has_reference: bool = conn
            .prepare("PRAGMA table_info(data)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|column| column.ok())
            .any(|name| name == "reference");
        if !has_reference {
            conn.execute_batch("ALTER TABLE data ADD COLUMN reference TEXT")?;
        }
    }
    conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
    Ok(())
}
