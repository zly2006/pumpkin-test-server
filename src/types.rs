use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub github: GitHubConfig,
    pub build: BuildConfig,
    pub runtime: RuntimeConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubConfig {
    pub repo_owner: String,
    pub repo_name: String,
    pub branch: String,
    pub check_interval: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BuildConfig {
    pub workspace_dir: String,
    pub binary_name: String,
    pub build_timeout: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub restart_delay: u64,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub data_file: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let content = std::fs::read_to_string("config.toml")?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCommit {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub date: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStatus {
    pub id: uuid::Uuid,
    pub commit_sha: String,
    pub status: BuildStatusType,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BuildStatusType {
    Pending,
    Building,
    Success,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub current_commit: Option<String>,
    pub build_status: BuildStatusType,
    pub is_running: bool,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub uptime: Option<chrono::Duration>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
}
