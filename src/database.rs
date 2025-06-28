use anyhow::Result;
use sqlx::{SqlitePool, Row};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::types::{BuildStatus, BuildStatusType, SystemStatus};

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        
        // 创建表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS builds (
                id TEXT PRIMARY KEY,
                commit_sha TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                error_message TEXT
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS system_status (
                id INTEGER PRIMARY KEY,
                current_commit TEXT,
                build_status TEXT NOT NULL,
                is_running BOOLEAN NOT NULL,
                last_check TEXT NOT NULL,
                started_at TEXT
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // 插入默认状态
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO system_status (id, build_status, is_running, last_check)
            VALUES (1, 'pending', false, ?)
            "#,
        )
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    pub async fn save_build_status(&self, build: &BuildStatus) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO builds (id, commit_sha, status, started_at, finished_at, error_message)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(build.id.to_string())
        .bind(&build.commit_sha)
        .bind(format!("{:?}", build.status).to_lowercase())
        .bind(build.started_at.to_rfc3339())
        .bind(build.finished_at.map(|dt| dt.to_rfc3339()))
        .bind(&build.error_message)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_latest_builds(&self, limit: i32) -> Result<Vec<BuildStatus>> {
        let rows = sqlx::query(
            r#"
            SELECT id, commit_sha, status, started_at, finished_at, error_message
            FROM builds
            ORDER BY started_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut builds = Vec::new();
        for row in rows {
            let status_str: String = row.get("status");
            let status = match status_str.as_str() {
                "pending" => BuildStatusType::Pending,
                "building" => BuildStatusType::Building,
                "success" => BuildStatusType::Success,
                "failed" => BuildStatusType::Failed,
                "stopped" => BuildStatusType::Stopped,
                _ => BuildStatusType::Pending,
            };

            builds.push(BuildStatus {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                commit_sha: row.get("commit_sha"),
                status,
                started_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("started_at"))?.with_timezone(&Utc),
                finished_at: row.get::<Option<String>, _>("finished_at")
                    .map(|s| DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)))
                    .transpose()?,
                error_message: row.get("error_message"),
            });
        }

        Ok(builds)
    }

    pub async fn update_system_status(&self, status: &SystemStatus) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE system_status
            SET current_commit = ?, build_status = ?, is_running = ?, last_check = ?
            WHERE id = 1
            "#,
        )
        .bind(&status.current_commit)
        .bind(format!("{:?}", status.build_status).to_lowercase())
        .bind(status.is_running)
        .bind(status.last_check.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_system_status(&self) -> Result<SystemStatus> {
        let row = sqlx::query(
            r#"
            SELECT current_commit, build_status, is_running, last_check, started_at
            FROM system_status
            WHERE id = 1
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let status_str: String = row.get("build_status");
        let status = match status_str.as_str() {
            "pending" => BuildStatusType::Pending,
            "building" => BuildStatusType::Building,
            "success" => BuildStatusType::Success,
            "failed" => BuildStatusType::Failed,
            "stopped" => BuildStatusType::Stopped,
            _ => BuildStatusType::Pending,
        };

        let uptime = row.get::<Option<String>, _>("started_at")
            .map(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|started| Utc::now().signed_duration_since(started.with_timezone(&Utc)))
                    .ok()
            })
            .flatten();

        Ok(SystemStatus {
            current_commit: row.get("current_commit"),
            build_status: status,
            is_running: row.get("is_running"),
            last_check: DateTime::parse_from_rfc3339(&row.get::<String, _>("last_check"))?.with_timezone(&Utc),
            uptime,
        })
    }

    pub async fn set_service_started(&self) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE system_status
            SET started_at = ?, is_running = true
            WHERE id = 1
            "#,
        )
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn set_service_stopped(&self) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE system_status
            SET is_running = false
            WHERE id = 1
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
