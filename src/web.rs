use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, services::ServeDir};

use crate::storage::Storage;
use crate::types::SystemStatus;

pub struct WebServer {
    app: Router,
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<RwLock<Storage>>,
}

#[derive(Deserialize)]
pub struct LogQuery {
    limit: Option<usize>,
    lang: Option<String>,
}

#[derive(Deserialize)]
pub struct IndexQuery {
    lang: Option<String>,
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl WebServer {
    pub fn new(storage: Arc<RwLock<Storage>>) -> Result<Self> {
        let state = AppState { storage };

        let app = Router::new()
            .route("/", get(index))
            .route("/api/status", get(get_status))
            .route("/api/builds", get(get_builds))
            .route("/api/restart", post(restart_service))
            .nest_service("/static", ServeDir::new("static"))
            .layer(CorsLayer::permissive())
            .with_state(state);

        Ok(Self { app })
    }

    pub fn router(self) -> Router {
        self.app
    }
}

async fn index(
    State(state): State<AppState>,
    Query(params): Query<IndexQuery>,
) -> Result<Html<String>, (StatusCode, String)> {
    let storage = state.storage.read().await;
    let status = storage.get_system_status();
    let builds = storage.get_latest_builds(10);
    
    let lang = params.lang.as_deref().unwrap_or("zh");
    let html = create_html_page(&status, &builds, lang);
    Ok(Html(html))
}

async fn get_status(State(state): State<AppState>) -> Result<Json<ApiResponse<SystemStatus>>, (StatusCode, String)> {
    let storage = state.storage.read().await;
    let status = storage.get_system_status();

    Ok(Json(ApiResponse {
        success: true,
        data: Some(status),
        error: None,
    }))
}

async fn get_builds(
    State(state): State<AppState>,
    Query(params): Query<LogQuery>,
) -> Result<Json<ApiResponse<Vec<crate::types::BuildStatus>>>, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(50).min(100);
    
    let storage = state.storage.read().await;
    let builds = storage.get_latest_builds(limit);

    Ok(Json(ApiResponse {
        success: true,
        data: Some(builds),
        error: None,
    }))
}

async fn restart_service(State(_state): State<AppState>) -> Result<Json<ApiResponse<String>>, (StatusCode, String)> {
    // 这里应该触发重启逻辑，暂时返回成功
    Ok(Json(ApiResponse {
        success: true,
        data: Some("Restart request received".to_string()),
        error: None,
    }))
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn create_html_page(
    status: &crate::types::SystemStatus,
    builds: &[crate::types::BuildStatus],
    lang: &str,
) -> String {
    let is_chinese = lang == "zh";
    
    // Language strings
    let (title, subtitle, running_status_label, build_status_label, current_commit_label, uptime_label, 
         build_history_label, refresh_btn_text, auto_refresh_text, no_builds_text, lang_switch_text,
         running_text, stopped_text, building_text, success_text, failed_text, pending_text) = if is_chinese {
        ("Pumpkin Monitor", "自动化部署监控系统", "运行状态", "构建状态", "当前提交", "运行时长", 
         "构建历史", "刷新状态", "自动刷新已启用", "暂无构建记录", "English",
         "运行中", "已停止", "构建中", "成功", "失败", "等待中")
    } else {
        ("Pumpkin Monitor", "Automated Deployment Monitoring System", "Running Status", "Build Status", "Current Commit", "Uptime",
         "Build History", "Refresh Status", "Auto refresh enabled", "No build records", "中文",
         "Running", "Stopped", "Building", "Success", "Failed", "Pending")
    };
    
    let running_class = if status.is_running { "status-running" } else { "status-stopped" };
    let build_class = format!("status-{:?}", status.build_status).to_lowercase();
    
    let running_status_text = if status.is_running { running_text } else { stopped_text };
    let build_status_text = match status.build_status {
        crate::types::BuildStatusType::Building => building_text,
        crate::types::BuildStatusType::Success => success_text,
        crate::types::BuildStatusType::Failed => failed_text,
        crate::types::BuildStatusType::Pending => pending_text,
        crate::types::BuildStatusType::Stopped => stopped_text,
    };
    
    let current_commit = status.current_commit.as_deref().unwrap_or("Unknown")[..8].to_string();
    let uptime = if let Some(uptime) = status.uptime {
        format!("{}d {}h {}m", 
            uptime.num_days(), 
            uptime.num_hours() % 24, 
            uptime.num_minutes() % 60)
    } else {
        "Unknown".to_string()
    };
    
    let builds_html = if builds.is_empty() {
        format!(r#"<p style="text-align: center; color: #666; padding: 40px;">{}</p>"#, no_builds_text)
    } else {
        builds.iter().map(|build| {
            let status_text = match build.status {
                crate::types::BuildStatusType::Building => building_text,
                crate::types::BuildStatusType::Success => success_text,
                crate::types::BuildStatusType::Failed => failed_text,
                crate::types::BuildStatusType::Pending => pending_text,
                crate::types::BuildStatusType::Stopped => stopped_text,
            };
            let status_class = format!("status-{:?}", build.status).to_lowercase();
            let error_html = if let Some(ref error) = build.error_message {
                format!(r#"<div class="error-message">{}</div>"#, html_escape(error))
            } else {
                String::new()
            };
            
            format!(r#"
                <div class="build-item">
                    <div class="build-header">
                        <span class="commit-sha">{}</span>
                        <span class="build-status {}">{}</span>
                    </div>
                    <div class="build-time">{}</div>
                    {}
                </div>
            "#, 
            &build.commit_sha[..8], 
            status_class, 
            status_text,
            build.started_at.format("%Y-%m-%d %H:%M:%S UTC"),
            error_html)
        }).collect::<String>()
    };
    
    let other_lang = if is_chinese { "en" } else { "zh" };
    let lang_attr = if is_chinese { "zh-CN" } else { "en" };

    format!(r#"<!DOCTYPE html>
<html lang="{}">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            color: #333;
        }}

        .container {{
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
        }}

        .header {{
            text-align: center;
            margin-bottom: 40px;
            color: white;
            position: relative;
        }}

        .header h1 {{
            font-size: 3rem;
            margin-bottom: 10px;
            text-shadow: 2px 2px 4px rgba(0,0,0,0.3);
        }}

        .header p {{
            font-size: 1.2rem;
            opacity: 0.9;
        }}

        .lang-switch {{
            position: absolute;
            top: 0;
            right: 0;
            background: rgba(255,255,255,0.2);
            border: 1px solid rgba(255,255,255,0.3);
            color: white;
            padding: 8px 16px;
            border-radius: 20px;
            cursor: pointer;
            text-decoration: none;
            font-size: 0.9rem;
            transition: all 0.3s;
        }}

        .lang-switch:hover {{
            background: rgba(255,255,255,0.3);
            transform: translateY(-2px);
        }}

        .status-card {{
            background: white;
            border-radius: 20px;
            padding: 30px;
            margin-bottom: 30px;
            box-shadow: 0 10px 30px rgba(0,0,0,0.1);
            backdrop-filter: blur(10px);
        }}

        .status-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 20px;
            margin-bottom: 30px;
        }}

        .status-item {{
            text-align: center;
            padding: 20px;
            background: linear-gradient(145deg, #f0f0f0, #ffffff);
            border-radius: 15px;
            box-shadow: 5px 5px 15px rgba(0,0,0,0.1);
        }}

        .status-item h3 {{
            color: #666;
            font-size: 0.9rem;
            text-transform: uppercase;
            letter-spacing: 1px;
            margin-bottom: 10px;
        }}

        .status-value {{
            font-size: 1.5rem;
            font-weight: bold;
            margin-bottom: 5px;
        }}

        .status-running {{ color: #28a745; }}
        .status-stopped {{ color: #dc3545; }}
        .status-building {{ color: #ffc107; }}
        .status-success {{ color: #28a745; }}
        .status-failed {{ color: #dc3545; }}
        .status-pending {{ color: #6c757d; }}

        .builds-section {{
            background: white;
            border-radius: 20px;
            padding: 30px;
            box-shadow: 0 10px 30px rgba(0,0,0,0.1);
        }}

        .builds-section h2 {{
            margin-bottom: 20px;
            color: #333;
            border-bottom: 2px solid #667eea;
            padding-bottom: 10px;
        }}

        .build-item {{
            background: #f8f9fa;
            border-radius: 10px;
            padding: 15px;
            margin-bottom: 15px;
            border-left: 4px solid #667eea;
            transition: transform 0.2s;
        }}

        .build-item:hover {{
            transform: translateX(5px);
        }}

        .build-header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 10px;
        }}

        .commit-sha {{
            font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', monospace;
            background: #e9ecef;
            padding: 2px 8px;
            border-radius: 4px;
            font-size: 0.9rem;
        }}

        .build-time {{
            color: #666;
            font-size: 0.9rem;
        }}

        .build-status {{
            padding: 4px 12px;
            border-radius: 20px;
            font-size: 0.8rem;
            font-weight: bold;
            text-transform: uppercase;
        }}

        .error-message {{
            background: #f8d7da;
            color: #721c24;
            padding: 10px;
            border-radius: 5px;
            margin-top: 10px;
            font-family: monospace;
            font-size: 0.9rem;
        }}

        .refresh-btn {{
            background: linear-gradient(145deg, #667eea, #764ba2);
            color: white;
            border: none;
            padding: 12px 24px;
            border-radius: 25px;
            cursor: pointer;
            font-size: 1rem;
            font-weight: bold;
            transition: all 0.3s;
            box-shadow: 0 4px 15px rgba(102, 126, 234, 0.4);
            margin-right: 10px;
        }}

        .refresh-btn:hover {{
            transform: translateY(-2px);
            box-shadow: 0 6px 20px rgba(102, 126, 234, 0.6);
        }}

        .refresh-btn:disabled {{
            opacity: 0.6;
            cursor: not-allowed;
            transform: none;
        }}

        .auto-refresh {{
            text-align: center;
            margin-top: 20px;
            color: #666;
        }}

        .refresh-indicator {{
            display: inline-block;
            width: 12px;
            height: 12px;
            border-radius: 50%;
            background: #28a745;
            margin-left: 8px;
            animation: pulse 2s infinite;
        }}

        @keyframes pulse {{
            0% {{ opacity: 1; transform: scale(1); }}
            50% {{ opacity: 0.5; transform: scale(1.1); }}
            100% {{ opacity: 1; transform: scale(1); }}
        }}

        .building {{
            animation: pulse 2s infinite;
        }}

        @media (max-width: 768px) {{
            .header h1 {{
                font-size: 2rem;
            }}
            
            .status-grid {{
                grid-template-columns: 1fr;
            }}
            
            .build-header {{
                flex-direction: column;
                align-items: flex-start;
                gap: 10px;
            }}

            .lang-switch {{
                position: static;
                margin-bottom: 20px;
                display: inline-block;
            }}
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <a href="/?lang={}" class="lang-switch">{}</a>
            <h1>🎃 {}</h1>
            <p>{}</p>
        </div>

        <div class="status-card">
            <div class="status-grid">
                <div class="status-item">
                    <h3>{}</h3>
                    <div class="status-value {}" id="running-status">
                        {}
                    </div>
                </div>
                
                <div class="status-item">
                    <h3>{}</h3>
                    <div class="status-value {}" id="build-status">
                        {}
                    </div>
                </div>
                
                <div class="status-item">
                    <h3>{}</h3>
                    <div class="status-value">
                        <span class="commit-sha" id="current-commit">{}</span>
                    </div>
                </div>
                
                <div class="status-item">
                    <h3>{}</h3>
                    <div class="status-value" id="uptime">
                        {}
                    </div>
                </div>
            </div>
            
            <div style="text-align: center;">
                <button class="refresh-btn" id="refresh-btn" onclick="refreshData()">{}</button>
                <span class="auto-refresh" id="auto-refresh-status">
                    {}<span class="refresh-indicator"></span>
                </span>
            </div>
        </div>

        <div class="builds-section">
            <h2>📋 {}</h2>
            <div id="builds-container">
                {}
            </div>
        </div>
    </div>

    <script>
        let refreshInterval;
        let currentLang = '{}';
        
        const translations = {{
            'zh': {{
                'running': '运行中',
                'stopped': '已停止',
                'building': '构建中',
                'success': '成功',
                'failed': '失败',
                'pending': '等待中',
                'refresh_status': '刷新状态',
                'refreshing': '刷新中...',
                'auto_refresh_enabled': '自动刷新已启用',
                'no_builds': '暂无构建记录'
            }},
            'en': {{
                'running': 'Running',
                'stopped': 'Stopped',
                'building': 'Building',
                'success': 'Success',
                'failed': 'Failed',
                'pending': 'Pending',
                'refresh_status': 'Refresh Status',
                'refreshing': 'Refreshing...',
                'auto_refresh_enabled': 'Auto refresh enabled',
                'no_builds': 'No build records'
            }}
        }};
        
        function t(key) {{
            return translations[currentLang][key] || key;
        }}

        async function refreshData() {{
            const refreshBtn = document.getElementById('refresh-btn');
            refreshBtn.disabled = true;
            refreshBtn.textContent = t('refreshing');
            
            try {{
                // Fetch status
                const statusResponse = await fetch('/api/status');
                const statusData = await statusResponse.json();
                
                // Fetch builds
                const buildsResponse = await fetch('/api/builds?limit=10');
                const buildsData = await buildsResponse.json();
                
                if (statusData.success && buildsData.success) {{
                    updateStatus(statusData.data);
                    updateBuilds(buildsData.data);
                }}
            }} catch (error) {{
                console.error('Refresh failed:', error);
            }} finally {{
                refreshBtn.disabled = false;
                refreshBtn.textContent = t('refresh_status');
            }}
        }}
        
        function updateStatus(status) {{
            const runningStatus = document.getElementById('running-status');
            const buildStatus = document.getElementById('build-status');
            const currentCommit = document.getElementById('current-commit');
            const uptime = document.getElementById('uptime');
            
            // Update running status
            runningStatus.textContent = status.is_running ? t('running') : t('stopped');
            runningStatus.className = 'status-value ' + (status.is_running ? 'status-running' : 'status-stopped');
            
            // Update build status
            const buildStatusText = t(status.build_status.toLowerCase());
            buildStatus.textContent = buildStatusText;
            buildStatus.className = 'status-value status-' + status.build_status.toLowerCase();
            
            // Update current commit
            currentCommit.textContent = status.current_commit ? status.current_commit.substring(0, 8) : 'Unknown';
            
            // Update uptime
            if (status.uptime) {{
                const days = Math.floor(status.uptime.secs / 86400);
                const hours = Math.floor((status.uptime.secs % 86400) / 3600);
                const minutes = Math.floor((status.uptime.secs % 3600) / 60);
                uptime.textContent = `${{days}}d ${{hours}}h ${{minutes}}m`;
            }} else {{
                uptime.textContent = 'Unknown';
            }}
        }}
        
        function updateBuilds(builds) {{
            const container = document.getElementById('builds-container');
            
            if (!builds || builds.length === 0) {{
                container.innerHTML = `<p style="text-align: center; color: #666; padding: 40px;">${{t('no_builds')}}</p>`;
                return;
            }}
            
            const buildsHtml = builds.map(build => {{
                const statusText = t(build.status.toLowerCase());
                const statusClass = 'status-' + build.status.toLowerCase();
                const errorHtml = build.error_message ? 
                    `<div class="error-message">${{build.error_message}}</div>` : '';
                const buildTime = new Date(build.started_at).toLocaleString();
                
                return `
                    <div class="build-item">
                        <div class="build-header">
                            <span class="commit-sha">${{build.commit_sha.substring(0, 8)}}</span>
                            <span class="build-status ${{statusClass}}">${{statusText}}</span>
                        </div>
                        <div class="build-time">${{buildTime}}</div>
                        ${{errorHtml}}
                    </div>
                `;
            }}).join('');
            
            container.innerHTML = buildsHtml;
        }}
        
        // Start auto refresh
        function startAutoRefresh() {{
            refreshInterval = setInterval(refreshData, 30000);
        }}
        
        // Initialize
        startAutoRefresh();
        
        // Refresh on visibility change
        document.addEventListener('visibilitychange', function() {{
            if (!document.hidden) {{
                refreshData();
            }}
        }});
    </script>
</body>
</html>"#,
        lang_attr, title, other_lang, lang_switch_text, title, subtitle,
        running_status_label, running_class, running_status_text,
        build_status_label, build_class, build_status_text,
        current_commit_label, current_commit,
        uptime_label, uptime,
        refresh_btn_text, auto_refresh_text,
        build_history_label, builds_html,
        lang
    )
}
