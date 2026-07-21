use log::info;
use roadwork_core::model::roadwork::Roadwork;
use roadwork_core::model::roadwork_data::RoadworkData;
use roadwork_sync::{Status, SyncData};
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
use std::path::Path;

pub struct SqliteStorage {
    pool: SqlitePool,
}

#[allow(dead_code)]
impl SqliteStorage {
    pub async fn new(data_dir: &Path) -> Self {
        let db_path = data_dir.join("roadworks.db");
        let url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .expect("Failed to open SQLite database");
        let storage = Self { pool };
        storage.init_schema().await;
        storage
    }

    async fn init_schema(&self) {
        sqlx::raw_sql(
            "CREATE TABLE IF NOT EXISTS roadworks (
                id TEXT NOT NULL,
                service TEXT NOT NULL,
                latitude REAL NOT NULL,
                longitude REAL NOT NULL,
                polygons_json TEXT,
                start INTEGER NOT NULL,
                \"end\" INTEGER NOT NULL,
                road TEXT,
                location_details TEXT,
                impact_circulation_detail TEXT,
                description TEXT,
                url TEXT NOT NULL,
                local_update_time INTEGER NOT NULL DEFAULT 0,
                server_update_time INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'New',
                dirty INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (id, service)
            );
            CREATE TABLE IF NOT EXISTS cache_meta (
                service TEXT PRIMARY KEY,
                created INTEGER NOT NULL
            );",
        )
        .execute(&self.pool)
        .await
        .expect("Failed to create schema");
    }

    pub async fn load_cache(&self, service_name: &str) -> Option<RoadworkData> {
        let created_row: (i64,) =
            sqlx::query_as("SELECT created FROM cache_meta WHERE service = ?")
                .bind(service_name)
                .fetch_one(&self.pool)
                .await
                .ok()?;
        let created = created_row.0 as u64;

        let rows: Vec<(
            String,
            f64,
            f64,
            Option<String>,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            i64,
            i64,
            String,
            i32,
        )> = sqlx::query_as(
            "SELECT id, latitude, longitude, polygons_json, start, end,
                    road, location_details, impact_circulation_detail,
                    description, url, local_update_time, server_update_time,
                    status, dirty
             FROM roadworks WHERE service = ?",
        )
        .bind(service_name)
        .fetch_all(&self.pool)
        .await
        .ok()?;

        if rows.is_empty() {
            return None;
        }

        let roadworks: HashMap<String, Roadwork> = rows
            .into_iter()
            .filter_map(|row| {
                let status = Status::from(&row.13);
                let polygons = row.3.as_deref().and_then(|s| serde_json::from_str(s).ok());

                let roadwork = Roadwork {
                    id: row.0.clone(),
                    latitude: row.1,
                    longitude: row.2,
                    polygons,
                    start: row.4,
                    end: row.5,
                    road: row.6,
                    location_details: row.7,
                    impact_circulation_detail: row.8,
                    description: row.9,
                    url: row.10,
                    sync_data: SyncData {
                        local_update_time: row.11 as u64,
                        server_update_time: row.12 as u64,
                        status,
                        dirty: row.14 != 0,
                    },
                };
                Some((row.0, roadwork))
            })
            .collect();

        Some(RoadworkData {
            source: service_name.to_string(),
            roadworks,
            created,
        })
    }

    pub async fn save_cache(
        &self,
        service_name: &str,
        data: &RoadworkData,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM roadworks WHERE service = ?")
            .bind(service_name)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM cache_meta WHERE service = ?")
            .bind(service_name)
            .execute(&mut *tx)
            .await?;

        for roadwork in data.roadworks.values() {
            let polygons_json = roadwork
                .polygons
                .as_ref()
                .and_then(|p| serde_json::to_string(p).ok());

            sqlx::query(
                "INSERT INTO roadworks (id, service, latitude, longitude, polygons_json,
                    start, end, road, location_details, impact_circulation_detail,
                    description, url, local_update_time, server_update_time, status, dirty)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&roadwork.id)
            .bind(service_name)
            .bind(roadwork.latitude)
            .bind(roadwork.longitude)
            .bind(&polygons_json)
            .bind(roadwork.start)
            .bind(roadwork.end)
            .bind(&roadwork.road)
            .bind(&roadwork.location_details)
            .bind(&roadwork.impact_circulation_detail)
            .bind(&roadwork.description)
            .bind(&roadwork.url)
            .bind(roadwork.sync_data.local_update_time() as i64)
            .bind(roadwork.sync_data.server_update_time() as i64)
            .bind(roadwork.sync_data.status.to_string())
            .bind(roadwork.sync_data.is_dirty() as i32)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query("INSERT INTO cache_meta (service, created) VALUES (?, ?)")
            .bind(service_name)
            .bind(data.created as i64)
            .execute(&mut *tx)
            .await?;

        info!(
            "Saved {} roadworks for {service_name}",
            data.roadworks.len()
        );

        tx.commit().await?;
        Ok(())
    }

    pub async fn delete_cache(&self, service_name: &str) {
        sqlx::query("DELETE FROM roadworks WHERE service = ?")
            .bind(service_name)
            .execute(&self.pool)
            .await
            .ok();
        sqlx::query("DELETE FROM cache_meta WHERE service = ?")
            .bind(service_name)
            .execute(&self.pool)
            .await
            .ok();
        info!("Deleted cache for {service_name}");
    }

    pub async fn load_roadwork(&self, service_name: &str, roadwork_id: &str) -> Option<Roadwork> {
        let row: (
            String,
            f64,
            f64,
            Option<String>,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            i64,
            i64,
            String,
            i32,
        ) = sqlx::query_as(
            "SELECT id, latitude, longitude, polygons_json, start, end,
                    road, location_details, impact_circulation_detail,
                    description, url, local_update_time, server_update_time,
                    status, dirty
             FROM roadworks WHERE service = ? AND id = ?",
        )
        .bind(service_name)
        .bind(roadwork_id)
        .fetch_optional(&self.pool)
        .await
        .ok()??;

        let status = (&row.13).into();
        let polygons = row.3.as_deref().and_then(|s| serde_json::from_str(s).ok());

        Some(Roadwork {
            id: row.0,
            latitude: row.1,
            longitude: row.2,
            polygons,
            start: row.4,
            end: row.5,
            road: row.6,
            location_details: row.7,
            impact_circulation_detail: row.8,
            description: row.9,
            url: row.10,
            sync_data: SyncData {
                local_update_time: row.11 as u64,
                server_update_time: row.12 as u64,
                status,
                dirty: row.14 != 0,
            },
        })
    }

    pub async fn save_roadwork(&self, service_name: &str, roadwork: &Roadwork) {
        let polygons_json = roadwork
            .polygons
            .as_ref()
            .and_then(|p| serde_json::to_string(p).ok());

        sqlx::query(
            "REPLACE INTO roadworks (id, service, latitude, longitude, polygons_json,
                start, end, road, location_details, impact_circulation_detail,
                description, url, local_update_time, server_update_time, status, dirty)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&roadwork.id)
        .bind(service_name)
        .bind(roadwork.latitude)
        .bind(roadwork.longitude)
        .bind(&polygons_json)
        .bind(roadwork.start)
        .bind(roadwork.end)
        .bind(&roadwork.road)
        .bind(&roadwork.location_details)
        .bind(&roadwork.impact_circulation_detail)
        .bind(&roadwork.description)
        .bind(&roadwork.url)
        .bind(roadwork.sync_data.local_update_time() as i64)
        .bind(roadwork.sync_data.server_update_time() as i64)
        .bind(roadwork.sync_data.status.to_string())
        .bind(roadwork.sync_data.is_dirty() as i32)
        .execute(&self.pool)
        .await
        .ok();
    }
}
