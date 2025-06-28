use anyhow::Result;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::process::Command as TokioCommand;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{info, warn, error};

use crate::types::{Config, BuildStatus, BuildStatusType, GitHubCommit};

pub struct BuildManager {
    config: Config,
    current_process: Option<Child>,
    workspace_path: PathBuf,
}

impl BuildManager {
    pub fn new(config: Config) -> Self {
        let workspace_path = PathBuf::from(&config.build.workspace_dir);
        
        Self {
            config,
            current_process: None,
            workspace_path,
        }
    }

    pub async fn ensure_workspace(&self) -> Result<()> {
        if !self.workspace_path.exists() {
            info!("Creating workspace directory: {:?}", self.workspace_path);
            fs::create_dir_all(&self.workspace_path).await?;
        }
        Ok(())
    }

    pub async fn clone_or_update_repo(&self) -> Result<()> {
        let repo_url = format!(
            "https://github.com/{}/{}.git",
            self.config.github.repo_owner,
            self.config.github.repo_name
        );

        let repo_path = self.workspace_path.join(&self.config.github.repo_name);

        if repo_path.exists() {
            info!("Updating existing repository");
            
            let mut child = TokioCommand::new("git")
                .args(&["pull", "origin", &self.config.github.branch])
                .current_dir(&repo_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;

            // 实时输出 git pull 的结果
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            
            let stdout_reader = BufReader::new(stdout);
            let stderr_reader = BufReader::new(stderr);
            
            let mut stdout_lines = stdout_reader.lines();
            let mut stderr_lines = stderr_reader.lines();
            
            let output_task = async {
                loop {
                    tokio::select! {
                        line = stdout_lines.next_line() => {
                            match line {
                                Ok(Some(line)) => {
                                    info!("[GIT] {}", line);
                                }
                                Ok(None) => break,
                                Err(_) => break,
                            }
                        }
                        line = stderr_lines.next_line() => {
                            match line {
                                Ok(Some(line)) => {
                                    info!("[GIT] {}", line);
                                }
                                Ok(None) => break,
                                Err(_) => break,
                            }
                        }
                    }
                }
            };

            let (_, exit_status) = tokio::join!(output_task, child.wait());
            
            if !exit_status?.success() {
                return Err(anyhow::anyhow!("Git pull failed"));
            }
        } else {
            info!("Cloning repository");
            
            let mut child = TokioCommand::new("git")
                .args(&["clone", "--branch", &self.config.github.branch, &repo_url])
                .current_dir(&self.workspace_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;

            // 实时输出 git clone 的结果
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            
            let stdout_reader = BufReader::new(stdout);
            let stderr_reader = BufReader::new(stderr);
            
            let mut stdout_lines = stdout_reader.lines();
            let mut stderr_lines = stderr_reader.lines();
            
            let output_task = async {
                loop {
                    tokio::select! {
                        line = stdout_lines.next_line() => {
                            match line {
                                Ok(Some(line)) => {
                                    info!("[GIT] {}", line);
                                }
                                Ok(None) => break,
                                Err(_) => break,
                            }
                        }
                        line = stderr_lines.next_line() => {
                            match line {
                                Ok(Some(line)) => {
                                    info!("[GIT] {}", line);
                                }
                                Ok(None) => break,
                                Err(_) => break,
                            }
                        }
                    }
                }
            };

            let (_, exit_status) = tokio::join!(output_task, child.wait());
            
            if !exit_status?.success() {
                return Err(anyhow::anyhow!("Git clone failed"));
            }
        }

        Ok(())
    }

    pub async fn build_project(&self, commit: &GitHubCommit) -> Result<BuildStatus> {
        let mut build_status = BuildStatus {
            id: uuid::Uuid::new_v4(),
            commit_sha: commit.sha.clone(),
            status: BuildStatusType::Building,
            started_at: chrono::Utc::now(),
            finished_at: None,
            error_message: None,
        };

        info!("Starting build for commit: {}", commit.sha);

        let repo_path = self.workspace_path.join(&self.config.github.repo_name);

        // 构建项目，使用实时输出
        let mut child = TokioCommand::new("cargo")
            .args(&["build", "--release"])
            .current_dir(&repo_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let timeout_duration = Duration::from_secs(self.config.build.build_timeout);
        
        // 创建输出读取任务
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        
        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);
        
        let mut stdout_lines = stdout_reader.lines();
        let mut stderr_lines = stderr_reader.lines();
        
        let mut error_output = String::new();
        
        // 实时读取输出
        let output_task = async {
            loop {
                tokio::select! {
                    line = stdout_lines.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                info!("[CARGO] {}", line);
                            }
                            Ok(None) => break,
                            Err(e) => {
                                warn!("Error reading stdout: {}", e);
                                break;
                            }
                        }
                    }
                    line = stderr_lines.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                warn!("[CARGO] {}", line);
                                error_output.push_str(&line);
                                error_output.push('\n');
                            }
                            Ok(None) => break,
                            Err(e) => {
                                warn!("Error reading stderr: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        };
        
        // 等待构建完成或超时
        let build_result = timeout(timeout_duration, async {
            tokio::join!(output_task, child.wait())
        }).await;
        
        match build_result {
            Ok((_, Ok(exit_status))) => {
                if exit_status.success() {
                    info!("Build successful for commit: {}", commit.sha);
                    build_status.status = BuildStatusType::Success;
                } else {
                    error!("Build failed for commit {}", commit.sha);
                    if !error_output.is_empty() {
                        error!("Build errors:\n{}", error_output);
                    }
                    build_status.status = BuildStatusType::Failed;
                    build_status.error_message = Some(error_output);
                }
            }
            Ok((_, Err(e))) => {
                error!("Build process error for commit {}: {}", commit.sha, e);
                build_status.status = BuildStatusType::Failed;
                build_status.error_message = Some(e.to_string());
            }
            Err(_) => {
                error!("Build timeout for commit: {}", commit.sha);
                build_status.status = BuildStatusType::Failed;
                build_status.error_message = Some("Build timeout".to_string());
                
                // 尝试杀死超时的进程
                let _ = child.kill().await;
            }
        }

        build_status.finished_at = Some(chrono::Utc::now());
        Ok(build_status)
    }

    pub fn stop_current_process(&mut self) -> Result<()> {
        if let Some(mut process) = self.current_process.take() {
            info!("Stopping current process");
            match process.kill() {
                Ok(_) => {
                    let _ = process.wait();
                    info!("Process stopped successfully");
                }
                Err(e) => {
                    warn!("Failed to kill process: {}", e);
                }
            }
        }
        Ok(())
    }

    pub fn start_new_process(&mut self) -> Result<u32> {
        let binary_path = self.workspace_path
            .join(&self.config.github.repo_name)
            .join("target")
            .join("release")
            .join(&self.config.build.binary_name);

        if !binary_path.exists() {
            return Err(anyhow::anyhow!("Binary not found: {:?}", binary_path));
        }

        info!("Starting new process: {:?}", binary_path);
        info!("Working directory: {:?}", self.workspace_path);

        // 在workspace目录中运行二进制文件
        // 让子进程继承父进程的stdio，避免终端状态问题
        let child = Command::new(&binary_path.canonicalize().unwrap())
            .current_dir(&self.workspace_path.canonicalize().unwrap())  // 设置工作目录为workspace
            .stdin(Stdio::null())   // 禁用stdin
            .stdout(Stdio::null()) // 继承stdout，避免管道阻塞
            .stderr(Stdio::null()) // 继承stderr，避免管道阻塞
            .spawn()?;

        let pid = child.id();
        self.current_process = Some(child);
        
        info!("New process started successfully in workspace with PID: {}", pid);
        
        Ok(pid)
    }

    pub fn is_process_running(&mut self) -> bool {
        if let Some(process) = &mut self.current_process {
            match process.try_wait() {
                Ok(Some(_)) => {
                    // 进程已结束
                    self.current_process = None;
                    false
                }
                Ok(None) => {
                    // 进程仍在运行
                    true
                }
                Err(_) => {
                    // 检查状态失败，假设进程已结束
                    self.current_process = None;
                    false
                }
            }
        } else {
            false
        }
    }

    pub fn is_repo_cloned(&self) -> bool {
        let repo_path = self.workspace_path.join(&self.config.github.repo_name);
        repo_path.exists() && repo_path.join(".git").exists()
    }

    pub fn is_binary_built(&self) -> bool {
        let binary_path = self.workspace_path
            .join(&self.config.github.repo_name)
            .join("target")
            .join("release")
            .join(&self.config.build.binary_name);
        binary_path.exists()
    }

    pub async fn restart_service(&mut self, commit: &GitHubCommit) -> Result<(BuildStatus, Option<u32>)> {
        let mut build_status = BuildStatus {
            id: uuid::Uuid::new_v4(),
            commit_sha: commit.sha.clone(),
            status: BuildStatusType::Building,
            started_at: chrono::Utc::now(),
            finished_at: None,
            error_message: None,
        };

        // 停止当前进程
        self.stop_current_process()?;

        // 等待一段时间
        tokio::time::sleep(Duration::from_secs(self.config.runtime.restart_delay)).await;

        // 更新代码
        if let Err(e) = self.clone_or_update_repo().await {
            build_status.status = BuildStatusType::Failed;
            build_status.error_message = Some(format!("Failed to update repository: {}", e));
            build_status.finished_at = Some(chrono::Utc::now());
            return Ok((build_status, None));
        }

        // 构建项目
        build_status = self.build_project(commit).await?;
        
        if build_status.status != BuildStatusType::Success {
            return Ok((build_status, None));
        }

        // 准备workspace配置
        if let Err(e) = self.prepare_workspace_config().await {
            warn!("Failed to prepare workspace config: {}", e);
        }

        // 启动新进程
        let pid = match self.start_new_process() {
            Ok(pid) => {
                build_status.finished_at = Some(chrono::Utc::now());
                info!("Service started with PID: {}", pid);
                Some(pid)
            }
            Err(e) => {
                build_status.status = BuildStatusType::Failed;
                build_status.error_message = Some(format!("Failed to start new process: {}", e));
                build_status.finished_at = Some(chrono::Utc::now());
                None
            }
        };

        Ok((build_status, pid))
    }

    pub async fn prepare_workspace_config(&self) -> Result<()> {
        // 在workspace中创建config.toml的副本
        let workspace_config_path = self.workspace_path.join("config.toml");
        
        if !workspace_config_path.exists() {
            info!("Creating config.toml in workspace");
            
            // 从当前目录复制config.toml到workspace
            if let Ok(config_content) = tokio::fs::read_to_string("config.toml").await {
                tokio::fs::write(&workspace_config_path, config_content).await?;
                info!("Config file copied to workspace: {:?}", workspace_config_path);
            } else {
                warn!("Could not find config.toml in current directory, process may need manual configuration");
            }
        }
        
        Ok(())
    }

    // 检查并清理可能存在的旧进程
    pub async fn cleanup_old_process(&self, pid: u32) -> Result<()> {
        info!("Checking for old process with PID: {}", pid);
        
        // 检查进程是否还存在
        let output = TokioCommand::new("ps")
            .args(&["-p", &pid.to_string()])
            .output()
            .await;
            
        match output {
            Ok(output) if output.status.success() => {
                // 进程还存在，尝试杀死它
                warn!("Found running process with PID {}, attempting to kill it", pid);
                
                let kill_output = TokioCommand::new("kill")
                    .args(&["-15", &pid.to_string()]) // 使用SIGTERM先尝试优雅关闭
                    .output()
                    .await;
                    
                match kill_output {
                    Ok(kill_output) if kill_output.status.success() => {
                        info!("Successfully sent SIGTERM to process {}", pid);
                        
                        // 等待3秒后检查进程是否还存在
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        
                        let check_output = TokioCommand::new("ps")
                            .args(&["-p", &pid.to_string()])
                            .output()
                            .await;
                            
                        if let Ok(check_output) = check_output {
                            if check_output.status.success() {
                                // 进程仍然存在，使用SIGKILL强制杀死
                                warn!("Process {} still running, using SIGKILL", pid);
                                let _ = TokioCommand::new("kill")
                                    .args(&["-9", &pid.to_string()])
                                    .output()
                                    .await;
                            }
                        }
                    }
                    _ => {
                        warn!("Failed to kill process {}", pid);
                    }
                }
            }
            _ => {
                info!("No process found with PID {}", pid);
            }
        }
        
        Ok(())
    }

    // 在启动前检查并清理旧进程
    pub async fn prepare_for_start(&self, storage: &Arc<RwLock<crate::storage::Storage>>) -> Result<()> {
        let current_status = {
            let storage_guard = storage.read().await;
            storage_guard.get_system_status()
        };
        
        if let Some(old_pid) = current_status.process_pid {
            self.cleanup_old_process(old_pid).await?;
        }
        
        Ok(())
    }
}
