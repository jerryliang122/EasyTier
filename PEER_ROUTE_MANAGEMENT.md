# Peer Route Management Feature

## 概述

此功能在EasyTier虚拟局域网中实现了自动的路由管理。当对端连接建立后，系统会自动在操作系统的路由表中添加一条路由，使流量能够正确路由到对端的公网IP地址。

## 功能描述

当启用此功能后，每当有新的对端连接时，系统会：

1. **获取对端信息**：从隧道信息中提取对端的公网IP地址
2. **获取本地网关**：自动检测本地网络的默认网关
3. **确定出接口**：根据目标IP确定本地使用的物理网络接口
4. **添加路由**：在系统路由表中添加一条/32主机路由，规则如下：
   - **网络目标**：对端的公网IP地址
   - **网络掩码**：32（单主机路由）
   - **网关**：本地计算机的默认网关
   - **接口**：物理网络接口的名称
   - **跃点数**：100

当对端断开连接时，系统会自动删除对应路由。

## 配置

### 通过命令行参数启用

在启动easytier时，添加 `--manage-peer-routes` 参数：

```bash
./easytier-core --manage-peer-routes
```

### 通过配置文件启用

在配置文件中添加：

```toml
manage_peer_routes = true
```

## 实现细节

### 新增文件

- `easytier/src/peers/peer_route_manager.rs`：路由管理器核心实现

### 修改文件

1. `easytier/src/peers/mod.rs`：添加路由管理器模块导出
2. `easytier/src/peers/peer_map.rs`：集成路由管理器
   - 添加 `route_manager` 字段
   - 在 `add_new_peer_conn` 中添加路由
   - 在 `close_peer` 中移除路由
3. `easytier/src/peers/peer_manager.rs`：根据配置启用路由管理
4. `easytier/src/proto/common.proto`：添加 `manage_peer_routes` 标志
5. `easytier/src/common/config.rs`：更新默认配置

### 支持的平台

- **Linux**：通过读取 `/proc/net/route` 获取默认网关
- **Windows**：通过 `route print` 命令获取默认网关
- **macOS/FreeBSD**：通过 `netstat -nr` 命令获取默认网关

## 路由管理器功能

### PeerRouteManager

主要的路由管理类，提供以下功能：

1. **添加对端路由**：`add_peer_route(peer_id, tunnel_info)`
   - 解析对端公网IP
   - 获取默认网关
   - 确定物理接口
   - 添加系统路由

2. **移除对端路由**：`remove_peer_routes(peer_id)`
   - 查找该对端的所有路由
   - 从系统删除路由

3. **列出路由**：`list_routes()`
   - 返回所有管理的路由信息

## 使用示例

### 启用路由管理

```bash
# 使用命令行参数
./easytier-core --manage-peer-routes -l udp://0.0.0.0:11010 -p udp://public-server.com:11010

# 或在配置文件中
cat > config.toml << EOF
manage_peer_routes = true
network_name = "my-vpn"
EOF
./easytier-core -c config.toml
```

### 查看路由表

```bash
# Linux
ip route show

# Windows
route print

# macOS/FreeBSD
netstat -nr
```

### 路由输出示例

当对端 `1.2.3.4` 连接后，系统路由表中会添加类似以下条目：

```
# Linux
1.2.3.4 via 192.168.1.1 dev eth0 metric 100

# Windows
1.2.3.4    255.255.255.255    192.168.1.1    192.168.1.100    100
```

## 注意事项

1. **权限要求**：添加/删除系统路由通常需要管理员/root权限
2. **网络稳定性**：频繁的路由变化可能会影响网络性能
3. **冲突处理**：如果路由已存在，系统可能会返回错误（会被日志记录）
4. **IP版本**：同时支持IPv4和IPv6路由
5. **默认禁用**：出于稳定性考虑，此功能默认禁用

## 故障排查

### 路由添加失败

如果看到 "Failed to add system route" 警告日志：

1. 检查是否有足够的权限（Linux需要sudo）
2. 确认网络接口名称正确
3. 检查目标IP是否有效

### 路由未生效

1. 确认路由已添加到系统路由表
2. 检查路由优先级（metric值）
3. 验证网络连通性

### 日志级别

启用调试日志以查看详细的路由操作：

```bash
RUST_LOG=debug ./easytier-core --manage-peer-routes
```

## 技术细节

### 获取默认网关

#### Linux
```rust
// 读取 /proc/net/route
// 查找 Destination=00000000（默认路由）
// 提取 Gateway 字段
```

#### Windows
```rust
// 执行 route print 0.0.0.0
// 解析输出获取网关地址
```

#### macOS/FreeBSD
```rust
// 执行 netstat -nr
// 查找 default 路由
// 提取网关地址
```

### 确定物理接口

```rust
// 创建UDP socket连接到目标IP
// 获取本地socket地址
// 通过IP地址查找对应的网络接口
```

## 性能考虑

- 路由操作通常在连接建立/断开时执行，不会影响数据转发性能
- 使用异步API确保不会阻塞主线程
- 错误处理确保即使路由操作失败，VPN连接仍能正常工作

## 未来改进

1. 支持自定义metric值
2. 支持路由持久化（重启后恢复）
3. 添加路由健康检查
4. 支持多网卡环境下的接口选择策略
5. 添加路由管理API接口

## 相关Issue

此功能解决了需要手动配置路由表的问题，特别适用于：
- 需要通过特定网关访问对端公网IP的场景
- 多网卡环境下的路由控制
- 特定网络拓扑要求
