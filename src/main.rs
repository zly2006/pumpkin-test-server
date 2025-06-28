mod types;
mod github;
mod build;
mod storage;
mod web;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use tracing::{info, error, warn};
use clap::Parser;

use types::{Config, BuildStatusType};
use github::GitHubMonitor;
use build::BuildManager;
use storage::Storage;
use web::WebServer;

#[derive(Parser)]
#[command(name = "pumpkin-monitor")]
#[command(about = "A monitoring system for Pumpkin-MC project")]
struct Args {
    #[arg(short, long, default_value = "config.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter("pumpkin_monitor=info,tower_http=debug")
        .init();

    let _args = Args::parse();
    
    // 加载配置
    let config = Config::load()?;
    info!("Configuration loaded successfully");

    // 初始化组件
    let mut github_monitor = GitHubMonitor::new(config.clone());
    let mut build_manager = BuildManager::new(config.clone());

    // 确保工作空间存在
    build_manager.ensure_workspace().await?;

    // 准备workspace配置
    build_manager.prepare_workspace_config().await?;

    // 初始化存储 - 将数据文件放在workspace中
    let workspace_data_file = std::path::Path::new(&config.build.workspace_dir)
        .join(&config.storage.data_file);
    let storage = Arc::new(RwLock::new(Storage::new(workspace_data_file.to_string_lossy().to_string()).await?));
    info!("Storage initialized in workspace: {:?}", workspace_data_file);

    // 检查并清理可能存在的旧进程
    build_manager.prepare_for_start(&storage).await?;

    // 启动 Web 服务器
    let web_server = WebServer::new(storage.clone())?;
    let addr = format!("{}:{}", config.server.host, config.server.port);
    
    info!("Starting web server on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, web_server.router()).await {
            error!("Web server error: {}", e);
        }
    });

    // 运行状态监控任务 - 每秒检查一次
    let storage_clone_status = storage.clone();
    let mut build_manager_clone = BuildManager::new(config.clone());
    let status_monitor_handle = tokio::spawn(async move {
        loop {
            match status_monitor_iteration(&mut build_manager_clone, &storage_clone_status).await {
                Ok(()) => {
                    // 状态监控成功，无需日志
                }
                Err(e) => {
                    warn!("Status monitor iteration failed: {}", e);
                }
            }
            
            // 每秒检查一次
            sleep(Duration::from_secs(1)).await;
        }
    });

    // 主监控循环 - 检查更新和构建
    let storage_clone = storage.clone();
    let monitor_handle = tokio::spawn(async move {
        let mut retry_count = 0;
        
        loop {
            match monitor_iteration(&mut github_monitor, &mut build_manager, &storage_clone).await {
                Ok(()) => {
                    retry_count = 0;
                    info!("Monitor iteration completed successfully");
                }
                Err(e) => {
                    retry_count += 1;
                    error!("Monitor iteration failed (attempt {}): {}", retry_count, e);
                    
                    if retry_count >= config.runtime.max_retries {
                        error!("Max retries reached, continuing with next iteration");
                        retry_count = 0;
                    }
                }
            }

            // 等待下次检查
            sleep(Duration::from_secs(config.github.check_interval)).await;
        }
    });

    info!("Pumpkin Monitor started successfully");
    info!("Web interface available at: http://{}", addr);

    // 等待任一任务完成
    tokio::select! {
        _ = server_handle => {
            warn!("Web server stopped");
        }
        _ = monitor_handle => {
            warn!("Monitor stopped");
        }
        _ = status_monitor_handle => {
            warn!("Status monitor stopped");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down");
        }
    }

    info!("Shutting down...");
    Ok(())
}

async fn monitor_iteration(
    github_monitor: &mut GitHubMonitor,
    build_manager: &mut BuildManager,
    storage: &Arc<RwLock<Storage>>,
) -> Result<()> {
    // 更新系统状态
    let is_running = build_manager.is_process_running();
    let current_status = {
        let storage_guard = storage.read().await;
        storage_guard.get_system_status()
    };
    
    let mut new_status = current_status.clone();
    new_status.is_running = is_running;
    new_status.last_check = chrono::Utc::now();
    
    {
        let mut storage_guard = storage.write().await;
        storage_guard.update_system_status(new_status.clone()).await?;
    }

    // 检查系统完整性
    let repo_cloned = build_manager.is_repo_cloned();
    let binary_built = build_manager.is_binary_built();
    let service_running = is_running;

    info!("System status check - Repo cloned: {}, Binary built: {}, Service running: {}", 
          repo_cloned, binary_built, service_running);

    // 检查新提交
    let mut needs_rebuild = false;
    let mut target_commit = None;

    if let Some(commit) = github_monitor.check_for_updates().await? {
        info!("New commit detected: {} by {}", commit.sha, commit.author);
        needs_rebuild = true;
        target_commit = Some(commit);
    } else {
        // 即使没有新提交，也要检查系统状态
        if !repo_cloned {
            info!("Repository not cloned, need to clone");
            needs_rebuild = true;
        } else if !binary_built {
            info!("Binary not built, need to build");
            needs_rebuild = true;
        }
        // 注意：不再在这里处理服务重启，由状态监控任务负责
    }

    // 如果需要重建或者有新提交
    if needs_rebuild {
        let commit = if let Some(c) = target_commit {
            c
        } else {
            // 如果没有新提交但需要重建，获取当前最新提交信息
            match github_monitor.get_latest_commit().await? {
                Some(c) => c,
                None => {
                    error!("Cannot get latest commit information");
                    return Err(anyhow::anyhow!("Failed to get latest commit"));
                }
            }
        };

        // 更新构建状态
        new_status.build_status = BuildStatusType::Building;
        new_status.current_commit = Some(commit.sha.clone());
        {
            let mut storage_guard = storage.write().await;
            storage_guard.update_system_status(new_status.clone()).await?;
        }

        // 重启服务
        let (build_result, new_pid) = build_manager.restart_service(&commit).await?;
        
        // 保存构建状态
        {
            let mut storage_guard = storage.write().await;
            storage_guard.save_build_status(build_result.clone()).await?;
        }

        match build_result.status {
            BuildStatusType::Success => {
                info!("Service restarted successfully for commit: {}", commit.sha);
                
                new_status.build_status = BuildStatusType::Success;
                if let Some(pid) = new_pid {
                    new_status.process_pid = Some(pid);
                }
                let mut storage_guard = storage.write().await;
                storage_guard.update_system_status(new_status).await?;
                storage_guard.set_service_started().await?;
            }
            _ => {
                error!("Failed to restart service: {:?}", build_result.error_message);
                
                new_status.build_status = BuildStatusType::Failed;
                new_status.process_pid = None;
                let mut storage_guard = storage.write().await;
                storage_guard.update_system_status(new_status).await?;
                storage_guard.set_service_stopped().await?;
            }
        }
    }

    Ok(())
}

async fn status_monitor_iteration(
    build_manager: &mut BuildManager,
    storage: &Arc<RwLock<Storage>>,
) -> Result<()> {
    let is_running = build_manager.is_process_running();
    
    // 获取当前状态
    let current_status = {
        let storage_guard = storage.read().await;
        storage_guard.get_system_status()
    };
    
    // 如果运行状态发生变化，更新存储
    if current_status.is_running != is_running {
        let mut new_status = current_status.clone();
        new_status.is_running = is_running;
        if is_running {
            new_status.build_status = BuildStatusType::Success;
        }
        new_status.last_check = chrono::Utc::now();
        
        if is_running {
            info!("Service started and is now running");
        } else {
            warn!("Service stopped unexpectedly");
        }
        
        let mut storage_guard = storage.write().await;
        storage_guard.update_system_status(new_status.clone()).await?;
        
        if !is_running {
            storage_guard.set_service_stopped().await?;
            // 清除PID信息
            let mut updated_status = new_status.clone();
            updated_status.process_pid = None;
            storage_guard.update_system_status(updated_status).await?;
        } else {
            storage_guard.set_service_started().await?;
        }
    }
    
    // 如果服务没有运行且没有正在构建，尝试重启
    if !is_running && current_status.build_status != BuildStatusType::Building {
        let repo_cloned = build_manager.is_repo_cloned();
        let binary_built = build_manager.is_binary_built();
        
        if repo_cloned && binary_built {
            info!("Attempting to restart service with existing binary");
            
            match build_manager.start_new_process() {
                Ok(pid) => {
                    info!("Service restarted successfully with PID: {}", pid);
                    let mut new_status = current_status.clone();
                    new_status.process_pid = Some(pid);
                    new_status.is_running = true;
                    
                    let mut storage_guard = storage.write().await;
                    storage_guard.update_system_status(new_status).await?;
                    storage_guard.set_service_started().await?;
                }
                Err(e) => {
                    warn!("Failed to restart service: {}", e);
                }
            }
        } else {
            // 如果没有仓库或二进制文件，记录但不尝试启动
            // 这种情况应该由主监控循环来处理
            if !repo_cloned {
                warn!("Cannot restart service: repository not cloned");
            } else if !binary_built {
                warn!("Cannot restart service: binary not built");
            }
        }
    }
    
    Ok(())
}
