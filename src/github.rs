use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use tracing::{info, warn};

use crate::types::{Config, GitHubCommit};

pub struct GitHubMonitor {
    client: Client,
    config: Config,
    last_commit_sha: Option<String>,
}

impl GitHubMonitor {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::new(),
            config,
            last_commit_sha: None,
        }
    }

    pub async fn check_for_updates(&mut self) -> Result<Option<GitHubCommit>> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/commits/{}",
            self.config.github.repo_owner,
            self.config.github.repo_name,
            self.config.github.branch
        );

        info!("Checking for updates: {}", url);

        let response = self.client
            .get(&url)
            .header("User-Agent", "pumpkin-monitor")
            .send()
            .await?;

        if !response.status().is_success() {
            warn!("GitHub API returned status: {}", response.status());
            return Ok(None);
        }

        let commit_data: Value = response.json().await?;
        
        let sha = commit_data["sha"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing commit SHA"))?
            .to_string();

        // 检查是否有新提交
        if let Some(ref last_sha) = self.last_commit_sha {
            if *last_sha == sha {
                return Ok(None);
            }
        }

        let commit = GitHubCommit {
            sha: sha.clone(),
            message: commit_data["commit"]["message"]
                .as_str()
                .unwrap_or("No message")
                .to_string(),
            author: commit_data["commit"]["author"]["name"]
                .as_str()
                .unwrap_or("Unknown")
                .to_string(),
            date: chrono::DateTime::parse_from_rfc3339(
                commit_data["commit"]["author"]["date"]
                    .as_str()
                    .unwrap_or("1970-01-01T00:00:00Z")
            )
            .unwrap_or_else(|_| chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z").unwrap())
            .with_timezone(&chrono::Utc),
        };

        self.last_commit_sha = Some(sha);
        info!("New commit found: {} by {}", commit.sha, commit.author);
        
        Ok(Some(commit))
    }

    pub async fn get_latest_commit(&self) -> Result<Option<GitHubCommit>> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/commits/{}",
            self.config.github.repo_owner,
            self.config.github.repo_name,
            self.config.github.branch
        );

        info!("Getting latest commit: {}", url);

        let response = self.client
            .get(&url)
            .header("User-Agent", "pumpkin-monitor")
            .send()
            .await?;

        if !response.status().is_success() {
            warn!("GitHub API returned status: {}", response.status());
            return Ok(None);
        }

        let commit_data: Value = response.json().await?;
        
        let sha = commit_data["sha"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing commit SHA"))?
            .to_string();

        let commit = GitHubCommit {
            sha: sha.clone(),
            message: commit_data["commit"]["message"]
                .as_str()
                .unwrap_or("No message")
                .to_string(),
            author: commit_data["commit"]["author"]["name"]
                .as_str()
                .unwrap_or("Unknown")
                .to_string(),
            date: chrono::DateTime::parse_from_rfc3339(
                commit_data["commit"]["author"]["date"]
                    .as_str()
                    .unwrap_or("1970-01-01T00:00:00Z")
            )
            .unwrap_or_else(|_| chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z").unwrap())
            .with_timezone(&chrono::Utc),
        };
        
        Ok(Some(commit))
    }

    pub fn set_last_commit(&mut self, sha: String) {
        self.last_commit_sha = Some(sha);
    }
}
