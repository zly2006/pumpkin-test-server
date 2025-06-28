#!/bin/bash

# Pumpkin Monitor 启动脚本

set -e

echo "🎃 Starting Pumpkin Monitor..."

# 检查 Rust 是否安装
if ! command -v cargo &> /dev/null; then
    echo "❌ Cargo not found. Please install Rust first."
    echo "Visit: https://rustup.rs/"
    exit 1
fi

# 检查配置文件
if [ ! -f "config.toml" ]; then
    echo "📝 Creating config.toml from example..."
    cp config.example.toml config.toml
    echo "✅ Please edit config.toml to match your settings"
fi

# 构建项目
echo "🔨 Building project..."
cargo build --release

# 创建必要的目录
mkdir -p workspace
mkdir -p static

# 启动服务
echo "🚀 Starting Pumpkin Monitor..."
exec ./target/release/pumpkin-monitor
