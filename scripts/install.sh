#!/bin/bash

# Pumpkin Monitor 安装脚本

set -e

echo "🎃 Installing Pumpkin Monitor..."

# 检查系统要求
if ! command -v git &> /dev/null; then
    echo "❌ Git not found. Please install Git first."
    exit 1
fi

if ! command -v cargo &> /dev/null; then
    echo "❌ Cargo not found. Please install Rust first."
    echo "Visit: https://rustup.rs/"
    exit 1
fi

# 安装依赖并构建
echo "📦 Installing dependencies..."
cargo build --release

# 创建必要的目录
mkdir -p workspace
mkdir -p static

# 设置权限
chmod +x scripts/start.sh

echo "✅ Installation completed!"
echo ""
echo "To start the monitor:"
echo "  ./scripts/start.sh"
echo ""
echo "Or run directly:"
echo "  cargo run --release"
echo ""
echo "Web interface will be available at: http://localhost:3000"
