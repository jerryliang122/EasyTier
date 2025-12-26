#!/bin/bash

# EasyTier 开发环境启动脚本
# 使用方法: ./start-dev.sh [command]

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 打印带颜色的消息
print_message() {
    local color=$1
    local message=$2
    echo -e "${color}${message}${NC}"
}

print_message $BLUE "🚀 启动 EasyTier 开发环境..."

# 检查 Docker 是否运行
if ! docker info > /dev/null 2>&1; then
    print_message $RED "❌ Docker 未运行，请先启动 Docker"
    exit 1
fi

# 检查 docker-compose 是否可用
if ! command -v docker-compose > /dev/null 2>&1 && ! docker compose version > /dev/null 2>&1; then
    print_message $RED "❌ docker-compose 未安装"
    exit 1
fi

# 进入项目根目录
cd "$(dirname "$0")/.."

print_message $YELLOW "📁 当前目录: $(pwd)"

# 构建或启动容器
if [ "$1" = "build" ] || [ "$1" = "rebuild" ]; then
    print_message $YELLOW "🔨 构建 Docker 镜像..."
    if [ "$1" = "rebuild" ]; then
        docker-compose -f .idc/docker-compose.yml build --no-cache
    else
        docker-compose -f .idc/docker-compose.yml build
    fi
    print_message $GREEN "✅ 镜像构建完成"
else
    # 检查镜像是否存在
    if ! docker images | grep -q "easytier-dev"; then
        print_message $YELLOW "🔨 Docker 镜像不存在，正在构建..."
        docker-compose -f .idc/docker-compose.yml build
        print_message $GREEN "✅ 镜像构建完成"
    fi
fi

# 启动容器
print_message $BLUE "🐳 启动开发容器..."

# 如果提供了额外命令，则执行该命令
if [ -n "$2" ]; then
    print_message $YELLOW "📝 执行命令: $2"
    docker-compose -f .idc/docker-compose.yml up --rm easytier-dev -c "$2"
else
    # 交互式会话
    docker-compose -f .idc/docker-compose.yml up --rm easytier-dev
fi

print_message $GREEN "🎉 开发环境已启动完成！"