# EasyTier 开发环境配置

## 项目依赖分析

### 主要技术栈
- **Rust**: 核心网络应用（版本 1.89.0+）
- **Node.js**: 前端开发（版本 22+）
- **pnpm**: 包管理器（版本 9.12.1+）
- **Vue.js**: 前端框架（版本 3.5.12+）
- **Tauri**: 跨平台 GUI 框架
- **Android SDK/NDK**: 移动端开发支持

### 项目结构
```
easytier/                    # Rust 核心库和应用
  ├── easytier-core          # 核心网络服务
  ├── easytier-cli           # 命令行工具
easytier-gui/                # Tauri GUI 应用
  ├── src/                   # Vue.js 前端代码
  ├── src-tauri/             # Tauri 后端
easytier-web/               # Web 前端
  ├── frontend/              # Web 应用
  ├── frontend-lib/          # 前端组件库
easytier-contrib/           # 扩展项目
  ├── easytier-ffi           # FFI 绑定
  ├── easytier-android-jni   # Android JNI
```

## Docker 环境配置

### 已配置的开发环境

**Dockerfile** 包含以下环境配置：

#### 1. Rust 环境
- 稳定版 Rust（1.89.0+）
- Rust Analyzer 代码分析
- Clippy 代码检查
- 多目标平台支持：
  - Linux (x86_64, aarch64)
  - Android (arm64, armv7, x86, x86_64)
  - MUSL 静态链接目标

#### 2. 前端开发环境
- Node.js 22 LTS
- pnpm 9.12.1+
- Vue.js 3.5.12
- TypeScript 5.6
- Vite 构建工具
- Tailwind CSS

#### 3. GUI 开发环境
- Tauri CLI
- GTK3 和 WebKit2GTK
- OpenSSL 和安全库
- 系统托盘支持

#### 4. Android 开发环境
- Android SDK 34
- Android NDK 26.1.10909125
- OpenJDK 21
- 构建工具 34.0.0
- Rust Android 目标支持

#### 5. 系统依赖
- Protobuf 编译器
- Clang/LLVM 工具链
- Jemalloc 内存分配器
- ZSTD 压缩库
- 网络工具

### 使用说明

#### 启动开发环境

```bash
# 构建 Docker 镜像
docker build -t easytier-dev -f .idc/dockerfile .

# 启动容器
docker run -it --name easytier-dev-workspace \
  -v $(pwd):/workspace \
  -p 11010-11012:11010-11012 \
  easytier-dev
```

#### 开发工作流

1. **Rust 开发**：
   ```bash
   cd easytier
   cargo build --release
   cargo test
   ```

2. **前端开发**：
   ```bash
   cd easytier-web/frontend
   pnpm install
   pnpm dev
   ```

3. **GUI 开发**：
   ```bash
   cd easytier-gui
   pnpm install
   pnpm tauri dev
   ```

4. **Android 开发**：
   ```bash
   cd easytier-contrib/easytier-android-jni
   cargo ndk build --target aarch64-linux-android
   ```

#### 端口映射
- `11010/tcp`: TCP 主端口
- `11010/udp`: UDP 主端口  
- `11011/udp`: WireGuard 端口
- `11011/tcp`: WebSocket 端口
- `11012/tcp`: WebSocket Secure 端口

#### 持久化数据
建议挂载以下目录以保持数据持久化：
- 项目源码：`-v $(pwd):/workspace`
- Cargo 缓存：`-v ~/.cargo:/root/.cargo`
- Node.js 缓存：`-v ~/.pnpm-store:/root/.pnpm-store`

### 特性说明

#### 完整的开发链
- 支持从底层 Rust 核心到上层 GUI 的完整开发链
- 集成了 Web、桌面、移动端的全栈开发环境

#### 跨平台构建
- 支持 Linux、Windows、macOS 的交叉编译
- Android 多架构构建支持
- MUSL 静态链接用于容器化部署

#### 优化的性能
- 使用 Jemalloc 优化内存分配
- ZSTD 压缩支持
- 零拷贝网络栈支持

### 故障排除

#### 常见问题

1. **权限问题**：确保 Docker 容器有足够的权限访问网络接口
2. **Android 构建**：检查 ANDROID_SDK_ROOT 和 NDK_HOME 环境变量
3. **内存不足**：Docker 容器建议至少 4GB 内存
4. **网络问题**：确保正确映射了所需端口

#### 调试模式

```bash
# 启用详细日志
export RUST_LOG=debug
export RUST_BACKTRACE=1

# 测试环境
cargo test --workspace
```

### 相关链接

- [EasyTier 官方文档](https://easytier.cn/)
- [Tauri 开发指南](https://tauri.app/)
- [Vue.js 文档](https://vuejs.org/)
- [Rust Book](https://doc.rust-lang.org/book/)