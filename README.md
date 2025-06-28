# Pumpkin Monitor

一个用于监控 [Pumpkin-MC](https://github.com/Pumpkin-MC/Pumpkin) 项目的自动部署系统。

## 功能特性

- 🔍 **自动监控** GitHub 仓库的新提交
- 🔨 **自动构建** Rust 项目的 release 版本
- 🔄 **自动重启** 服务实例（停止旧的，启动新的）
- 🌐 **Web 界面** 查看日志和状态
- 💾 **数据持久化** 使用 JSON 文件存储状态和构建历史
- 🛡️ **健壮设计** 应对随时重启和网络问题
- 📱 **响应式设计** 支持桌面和移动设备

## 快速开始

### 系统要求

- Rust 1.70+
- Git
- 网络连接

### 安装

```bash
# 克隆项目
git clone <your-repo-url>
cd pumpkin-monitor

# 运行安装脚本
./scripts/install.sh

# 或手动安装
cargo build --release
mkdir -p workspace static
```

### 配置

1. 复制配置文件：
```bash
cp config.example.toml config.toml
```

2. 编辑 `config.toml` 文件：
```toml
[server]
host = "0.0.0.0"
port = 3000

[github]
repo_owner = "Pumpkin-MC"
repo_name = "Pumpkin"
branch = "main"
check_interval = 300  # 检查间隔，秒

[build]
workspace_dir = "./workspace"
binary_name = "pumpkin"
build_timeout = 1800  # 构建超时，秒

[runtime]
restart_delay = 5  # 重启延迟，秒
max_retries = 3

[storage]
data_file = "./data.json"
```

### 运行

```bash
# 使用启动脚本
./scripts/start.sh

# 或直接运行
cargo run --release

# 或使用已构建的二进制文件
./target/release/pumpkin-monitor
```

### 访问 Web 界面

打开浏览器访问：`http://localhost:3000`

## Web 界面功能

### 首页
- 实时显示系统状态
- 当前运行状态
- 构建状态
- 当前提交信息
- 运行时长
- 构建历史记录

### API 接口

- `GET /` - 首页
- `GET /api/status` - 获取当前状态
- `GET /api/builds?limit=50` - 获取构建历史
- `POST /api/restart` - 手动重启（暂未实现）

## 系统架构

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   GitHub API    │ -> │  Pumpkin Monitor │ -> │  Target Binary  │
│   监控新提交     │    │    主程序        │    │   Pumpkin 实例  │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                               │
                               v
                       ┌──────────────────┐
                       │   Web Interface  │
                       │    状态展示      │
                       └──────────────────┘
```

### 核心组件

1. **GitHub Monitor** (`src/github.rs`)
   - 定期检查 GitHub API
   - 获取最新提交信息
   - 检测新的提交

2. **Build Manager** (`src/build.rs`)
   - 克隆/更新代码仓库
   - 执行 Cargo 构建
   - 管理进程生命周期

3. **Storage** (`src/storage.rs`)
   - JSON 文件数据持久化
   - 系统状态管理
   - 构建历史记录

4. **Web Server** (`src/web.rs`)
   - HTTP API 服务
   - 静态文件服务
   - 实时状态展示

## 部署到生产环境

### 使用 systemd (Linux)

1. 复制 service 文件：
```bash
sudo cp scripts/pumpkin-monitor.service /etc/systemd/system/
```

2. 创建用户和目录：
```bash
sudo useradd -r -s /bin/false pumpkin
sudo mkdir -p /opt/pumpkin-monitor
sudo chown pumpkin:pumpkin /opt/pumpkin-monitor
```

3. 复制文件：
```bash
sudo cp -r * /opt/pumpkin-monitor/
sudo chown -R pumpkin:pumpkin /opt/pumpkin-monitor
```

4. 启动服务：
```bash
sudo systemctl enable pumpkin-monitor
sudo systemctl start pumpkin-monitor
sudo systemctl status pumpkin-monitor
```

### 使用 Docker (可选)

创建 `Dockerfile`：
```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y git && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/pumpkin-monitor .
COPY config.toml .
EXPOSE 3000
CMD ["./pumpkin-monitor"]
```

构建和运行：
```bash
docker build -t pumpkin-monitor .
docker run -d -p 3000:3000 -v $(pwd)/data:/app/data pumpkin-monitor
```

## 开发

### 项目结构

```
├── src/
│   ├── main.rs          # 主程序入口
│   ├── types.rs         # 数据类型定义
│   ├── github.rs        # GitHub API 集成
│   ├── build.rs         # 构建管理
│   ├── storage.rs       # 数据存储
│   └── web.rs           # Web 服务器
├── scripts/
│   ├── install.sh       # 安装脚本
│   ├── start.sh         # 启动脚本
│   └── pumpkin-monitor.service  # systemd 服务文件
├── templates/           # Web 模板
├── static/             # 静态资源
├── config.toml         # 配置文件
└── data.json          # 数据文件
```

### 运行测试

```bash
cargo test
```

### 代码格式化

```bash
cargo fmt
```

### 代码检查

```bash
cargo clippy
```

## 故障排除

### 常见问题

1. **构建失败**
   - 检查 Rust 版本是否符合要求
   - 确保有足够的磁盘空间
   - 检查网络连接

2. **无法访问 GitHub API**
   - 检查网络连接
   - 考虑使用 GitHub Token（如果有频率限制）

3. **进程无法启动**
   - 检查二进制文件是否存在
   - 确认文件权限
   - 查看日志输出

4. **Web 界面无法访问**
   - 检查端口是否被占用
   - 确认防火墙设置
   - 查看服务状态

### 日志查看

```bash
# 如果使用 systemd
sudo journalctl -u pumpkin-monitor -f

# 如果直接运行
# 日志会输出到标准输出
```

## 贡献

欢迎提交 Issue 和 Pull Request！

## 许可证

MIT License

## 更新日志

### v0.1.0
- 初始版本
- 基本的监控和自动部署功能
- Web 界面
- JSON 数据存储
