use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::{info, warn};

use crate::types::{BuildStatus, BuildStatusType, SystemStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageData {
    pub builds: Vec<BuildStatus>,
    pub system_status: SystemStatus,
}

impl Default for StorageData {
    fn default() -> Self {
        Self {
            builds: Vec::new(),
            system_status: SystemStatus {
                current_commit: None,
                build_status: BuildStatusType::Pending,
                is_running: false,
                last_check: chrono::Utc::now(),
                uptime: None,
                started_at: None,
            },
        }
    }
}

pub struct Storage {
    file_path: String,
    data: StorageData,
}

impl Storage {
    pub async fn new(file_path: String) -> Result<Self> {
        let data = if Path::new(&file_path).exists() {
            let content = fs::read_to_string(&file_path).await?;
            match serde_json::from_str(&content) {
                Ok(data) => {
                    info!("Loaded existing data from {}", file_path);
                    data
                }
                Err(e) => {
                    warn!("Failed to parse existing data file: {}, using default", e);
                    StorageData::default()
                }
            }
        } else {
            info!("Creating new data file: {}", file_path);
            StorageData::default()
        };

        let mut storage = Self { file_path, data };
        storage.save().await?;
        
        Ok(storage)
    }

    pub async fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.data)?;
        fs::write(&self.file_path, json).await?;
        Ok(())
    }

    pub async fn save_build_status(&mut self, build: BuildStatus) -> Result<()> {
        // 移除相同 ID 的构建记录（如果存在）
        self.data.builds.retain(|b| b.id != build.id);
        
        // 添加新的构建记录
        self.data.builds.push(build);
        
        // 按时间排序，最新的在前面
        self.data.builds.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        
        // 只保留最近的100条记录
        if self.data.builds.len() > 100 {
            self.data.builds.truncate(100);
        }
        
        self.save().await?;
        Ok(())
    }

    pub fn get_latest_builds(&self, limit: usize) -> Vec<BuildStatus> {
        self.data.builds
            .iter()
            .take(limit)
            .cloned()
            .collect()
    }

    pub async fn update_system_status(&mut self, status: SystemStatus) -> Result<()> {
        self.data.system_status = status;
        self.save().await?;
        Ok(())
    }

    pub fn get_system_status(&self) -> SystemStatus {
        self.data.system_status.clone()
    }

    pub async fn set_service_started(&mut self) -> Result<()> {
        self.data.system_status.is_running = true;
        self.data.system_status.started_at = Some(chrono::Utc::now());
        self.save().await?;
        Ok(())
    }

    pub async fn set_service_stopped(&mut self) -> Result<()> {
        self.data.system_status.is_running = false;
        self.save().await?;
        Ok(())
    }
}
