# Tools MCP 设计文档

## 概述

`tools-mcp` 是一个统一的工具，支持通过 CLI 和 MCP 协议连接 SSH、MySQL、Redis 等服务。所有连接都支持通过 SSH 跳板机（jump host）访问内网资源。

## 架构设计

### 单一二进制，双模式运行

- **MCP 模式（默认）**：`tools-mcp` - 启动 MCP 服务器，通过 stdio 通信
- **CLI 模式**：`tools-mcp <service> <command>` - 执行单次命令并输出结果

两种模式共享相同的核心逻辑，只是输出格式不同：
- CLI 模式：人类可读的文本/表格格式
- MCP 模式：JSON-RPC 协议格式

### 核心模块

1. **Config 模块** - 配置管理
   - 加载 `~/.config/tools-mcp/config.toml`（默认 TOML 配置）
   - 加载 `--config` 指定的 YAML 配置文件
   - 解析命令行参数
   - 合并配置（按优先级：命令行 > YAML > 环境变量 > TOML profile > 默认值）

2. **Connection 模块** - 连接管理
   - `Connection` trait - 统一的连接接口
   - `MySQLConnection` - 使用 `mysql_async` crate
   - `RedisConnection` - 使用 `redis` crate
   - `SSHConnection` - 使用 `ssh2` crate

3. **Tunnel 模块** - 隧道管理
   - `Tunnel` trait - 统一的隧道接口
   - `DirectTunnel` - 直接连接（无隧道）
   - `SSHTunnel` - SSH 跳板机隧道
   - 未来可扩展：`VPNTunnel`, `ProxyTunnel` 等

4. **Executor 模块** - 命令执行
   - 统一的执行接口
   - 结果标准化

4. **Output 模块** - 输出格式化
   - CLI：表格/文本格式
   - MCP：JSON 格式

5. **MCP Server 模块** - MCP 协议实现
   - JSON-RPC 请求处理
   - 工具调用路由
   - 错误响应封装

## 命令结构

### 全局参数

所有子命令都支持以下全局参数：

**配置文件参数：**
```bash
--config <path>            # 指定 YAML 配置文件路径
```

**隧道参数：**
```bash
--tunnel <type>            # 隧道类型：direct, ssh (默认: direct)
```

**SSH 隧道参数（当 --tunnel=ssh 时）：**
```bash
--ssh-jump <host>          # 跳板机地址
--ssh-user <user>          # 跳板机用户名
--ssh-password <password>  # 跳板机密码
--ssh-key-path <path>      # 跳板机密钥路径
--ssh-port <port>          # 跳板机端口（默认 22）
```

### MySQL 命令

```bash
# 直接连接（默认）
tools-mcp mysql "SELECT * FROM users" \
  --host=localhost --port=3306 --user=root --password=secret

# 使用配置文件
tools-mcp mysql "SELECT * FROM users" --profile=prod

# 使用 YAML 配置文件
tools-mcp --config=./mysql-config.yaml mysql "SELECT * FROM users"

# 通过 SSH 跳板机连接内网数据库
tools-mcp --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=secret \
  mysql "SELECT * FROM users" \
  --host=mysql.internal.com --user=dbuser --password=dbpass

# YAML 配置文件 + SSH 隧道
tools-mcp --config=./mysql-config.yaml --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin \
  mysql "SELECT * FROM users"
```

**MySQL 参数：**
- `--host` - 数据库主机
- `--port` - 数据库端口（默认 3306）
- `--user` - 数据库用户名
- `--password` - 数据库密码
- `--database` - 数据库名称（可选）
- `--profile` - 使用配置文件中的 profile

### Redis 命令

```bash
# 直接连接
tools-mcp redis "GET key" --host=localhost --port=6379

# 通过 SSH 跳板机连接
tools-mcp --ssh-jump=bastion.com --ssh-key-path=~/.ssh/jump_key \
  redis "SET key value" --host=redis.internal.com --port=6379
```

**Redis 参数：**
- `--host` - Redis 主机
- `--port` - Redis 端口（默认 6379）
- `--password` - Redis 密码（可选）
- `--db` - 数据库编号（默认 0）
- `--profile` - 使用配置文件中的 profile

### SSH 命令

```bash
# 直接连接
tools-mcp ssh "ls -la" --host=server.com --user=admin --key-path=~/.ssh/id_rsa

# 通过跳板机连接（单级）
tools-mcp --ssh-jump=bastion.com --ssh-user=jump_admin --ssh-password=secret \
  ssh "ls -la" --host=target.internal.com --user=admin --key-path=~/.ssh/id_rsa

# 通过跳板机连接（多级）
tools-mcp --ssh-jump=bastion1.com,bastion2.com --ssh-user=jump_admin \
  ssh "cat /var/log/app.log" --host=target.com --user=admin
```

**SSH 参数：**
- `--host` - 目标主机
- `--port` - SSH 端口（默认 22）
- `--user` - 用户名
- `--password` - 密码（可选）
- `--key-path` - 私钥路径（可选）
- `--profile` - 使用配置文件中的 profile

## 配置文件

### 默认配置文件

默认配置文件位置：`~/.config/tools-mcp/config.toml`

### YAML 配置文件

除了默认的 TOML 配置文件，还支持通过 `--config` 参数指定 YAML 配置文件，用于临时或项目特定的配置。

**YAML 配置示例：**

```yaml
# mysql-config.yaml
type: mysql
host: mysql.internal.com
port: 3306
user: dbuser
password: secret
database: app_db

# 隧道配置
tunnel_type: ssh  # direct 或 ssh
ssh_jump: bastion.com
ssh_user: jump_admin
ssh_password: jump_secret
```

```yaml
# redis-config.yaml
type: redis
host: redis.internal.com
port: 6379
password: redis_pass
db: 0

ssh_jump: bastion.com
ssh_user: jump_admin
ssh_key_path: ~/.ssh/jump_key
```

```yaml
# ssh-config.yaml
type: ssh
host: target.internal.com
port: 22
user: admin
key_path: ~/.ssh/id_rsa

# 多级跳板机
ssh_jump: bastion1.com,bastion2.com
ssh_user: jump_admin
ssh_key_path: ~/.ssh/jump_key
```

**使用方式：**

```bash
# 使用 YAML 配置文件
tools-mcp --config=./mysql-config.yaml mysql "SELECT * FROM users"

# YAML 配置 + 命令行参数覆盖
tools-mcp --config=./mysql-config.yaml mysql "SELECT * FROM users" --database=other_db

# YAML 配置 + 全局 SSH 参数覆盖
tools-mcp --config=./mysql-config.yaml --ssh-jump=other-bastion.com \
  mysql "SELECT * FROM users"
```

### TOML 配置示例（Profile 模式）

```toml
# MySQL 配置
[profiles.prod-db]
type = "mysql"
host = "mysql.internal.com"
port = 3306
user = "dbuser"
password = "secret"
database = "app_db"

# 隧道配置
tunnel_type = "ssh"  # "direct" 或 "ssh"
ssh_jump = "bastion.com"
ssh_user = "jump_admin"
ssh_password = "jump_secret"  # 或使用 ssh_key_path

# Redis 配置
[profiles.cache]
type = "redis"
host = "redis.internal.com"
port = 6379
password = "redis_pass"
db = 0

# SSH 跳板机配置
ssh_jump = "bastion.com"
ssh_user = "jump_admin"
ssh_key_path = "~/.ssh/jump_key"

# SSH 服务器配置
[profiles.prod-server]
type = "ssh"
host = "target.internal.com"
port = 22
user = "admin"
key_path = "~/.ssh/id_rsa"

# 多级跳板机
ssh_jump = "bastion1.com,bastion2.com"
ssh_user = "jump_admin"
ssh_key_path = "~/.ssh/jump_key"

# 直接 SSH 连接（无跳板机）
[profiles.dev-server]
type = "ssh"
host = "dev.example.com"
user = "developer"
password = "dev_pass"
```

### 配置优先级

配置参数的优先级从高到低：

1. 命令行参数（最高优先级）
2. `--config` 指定的 YAML 配置文件
3. 环境变量
4. `--profile` 指定的 TOML profile
5. 默认值（最低优先级）

**示例：**

```bash
# YAML 文件中 host=mysql.internal.com, tunnel_type=ssh
# 命令行参数 --host=localhost --tunnel=direct
# 最终使用 localhost 和 direct 隧道（命令行参数优先级更高）
tools-mcp --config=./mysql-config.yaml --tunnel=direct mysql "SELECT 1" --host=localhost
```

## SSH 隧道实现

### Tunnel Trait 设计

所有隧道方式都实现统一的 `Tunnel` trait：

```rust
#[async_trait]
pub trait Tunnel: Send + Sync {
    async fn establish(&mut self) -> Result<TunnelEndpoint>;
    async fn close(&mut self) -> Result<()>;
    fn is_active(&self) -> bool;
}

pub struct TunnelEndpoint {
    pub host: String,
    pub port: u16,
}
```

### 隧道实现

**DirectTunnel** - 直接连接，无隧道：
```rust
pub struct DirectTunnel {
    target_host: String,
    target_port: u16,
}

impl Tunnel for DirectTunnel {
    async fn establish(&mut self) -> Result<TunnelEndpoint> {
        // 直接返回目标地址
        Ok(TunnelEndpoint {
            host: self.target_host.clone(),
            port: self.target_port,
        })
    }
}
```

**SSHTunnel** - SSH 跳板机隧道：
```rust
pub struct SSHTunnel {
    jump_host: String,
    jump_port: u16,
    jump_user: String,
    jump_auth: SSHAuth,
    target_host: String,
    target_port: u16,
    local_port: Option<u16>,
    ssh_session: Option<Session>,
}

impl Tunnel for SSHTunnel {
    async fn establish(&mut self) -> Result<TunnelEndpoint> {
        // 1. 连接到跳板机
        // 2. 创建端口转发：localhost:random_port -> target_host:target_port
        // 3. 返回 localhost:random_port
        Ok(TunnelEndpoint {
            host: "localhost".to_string(),
            port: self.local_port.unwrap(),
        })
    }
}
```

### 工作原理

对于 MySQL 和 Redis 连接：

1. 根据配置选择隧道类型（Direct 或 SSH）
2. 调用 `tunnel.establish()` 获取连接端点
3. 使用返回的端点连接 MySQL/Redis
4. 命令执行完成后调用 `tunnel.close()`

### 多级跳板机

支持通过多个跳板机级联连接：

```
Client -> Bastion1 -> Bastion2 -> Target
```

实现方式：
- 使用 SSH 的 ProxyJump 功能
- 或者递归建立隧道：先连接 Bastion1，再通过 Bastion1 连接 Bastion2，最后连接 Target

## 数据流

### CLI 模式

```
命令行参数 -> 参数解析 -> 加载配置 -> 合并配置
  -> 建立 SSH 隧道（如需要）
  -> 建立目标连接
  -> 执行命令
  -> 格式化输出（文本/表格）
  -> 关闭连接和隧道
```

### MCP 模式

```
启动 MCP 服务器 -> 监听 stdin
  -> 接收 JSON-RPC 请求
  -> 解析工具调用
  -> 执行命令（复用 CLI 逻辑）
  -> 返回 JSON-RPC 响应
```

## MCP 工具定义

### mysql_exec

执行 MySQL 查询。

**参数：**
- `query` (string, required) - SQL 查询语句
- `host` (string, optional) - 数据库主机
- `port` (number, optional) - 数据库端口
- `user` (string, optional) - 用户名
- `password` (string, optional) - 密码
- `database` (string, optional) - 数据库名
- `profile` (string, optional) - 配置 profile 名称
- `ssh_jump` (string, optional) - 跳板机地址
- `ssh_user` (string, optional) - 跳板机用户名
- `ssh_password` (string, optional) - 跳板机密码
- `ssh_key_path` (string, optional) - 跳板机密钥路径

**返回：**
```json
{
  "columns": ["id", "name", "email"],
  "rows": [
    [1, "Alice", "alice@example.com"],
    [2, "Bob", "bob@example.com"]
  ],
  "affected_rows": 2
}
```

### redis_exec

执行 Redis 命令。

**参数：**
- `command` (string, required) - Redis 命令
- `host` (string, optional) - Redis 主机
- `port` (number, optional) - Redis 端口
- `password` (string, optional) - Redis 密码
- `db` (number, optional) - 数据库编号
- `profile` (string, optional) - 配置 profile 名称
- `ssh_jump` (string, optional) - 跳板机地址
- `ssh_user` (string, optional) - 跳板机用户名
- `ssh_password` (string, optional) - 跳板机密码
- `ssh_key_path` (string, optional) - 跳板机密钥路径

**返回：**
```json
{
  "result": "OK",
  "type": "string"
}
```

### ssh_exec

执行 SSH 命令。

**参数：**
- `command` (string, required) - 要执行的命令
- `host` (string, optional) - 目标主机
- `port` (number, optional) - SSH 端口
- `user` (string, optional) - 用户名
- `password` (string, optional) - 密码
- `key_path` (string, optional) - 私钥路径
- `profile` (string, optional) - 配置 profile 名称
- `ssh_jump` (string, optional) - 跳板机地址（支持逗号分隔的多级）
- `ssh_user` (string, optional) - 跳板机用户名
- `ssh_password` (string, optional) - 跳板机密码
- `ssh_key_path` (string, optional) - 跳板机密钥路径

**返回：**
```json
{
  "stdout": "command output",
  "stderr": "",
  "exit_code": 0
}
```

## 错误处理

### 错误类型

1. **配置错误** - 配置文件格式错误、缺少必需参数
2. **连接错误** - 无法连接到服务器、认证失败、超时
3. **执行错误** - SQL 语法错误、Redis 命令错误、SSH 命令执行失败
4. **隧道错误** - SSH 隧道建立失败、端口转发失败

### 处理策略

**CLI 模式：**
- 打印友好的错误信息到 stderr
- 返回非零退出码
- 错误信息包含问题描述和可能的解决方案

**MCP 模式：**
- 将错误包装为 MCP 错误响应
- 包含错误码和详细信息
- 错误码遵循 JSON-RPC 规范

**通用原则：**
- 使用 `anyhow` 处理错误链，保留完整的错误上下文
- 敏感信息（密码）不出现在错误消息中
- 提供可操作的错误提示

### 错误示例

```rust
// CLI 模式错误输出
Error: Failed to connect to MySQL server
  Caused by: Connection refused (os error 111)
  
Possible solutions:
  - Check if MySQL server is running
  - Verify host and port are correct
  - Check firewall settings

// MCP 模式错误响应
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32000,
    "message": "Failed to connect to MySQL server",
    "data": {
      "type": "ConnectionError",
      "details": "Connection refused"
    }
  }
}
```

## 测试策略

### 单元测试

- **Config 模块**：配置文件解析、参数合并、默认值处理
- **Connection 模块**：使用 mock 测试连接逻辑
- **Output 模块**：测试不同格式的输出转换
- **TunnelManager**：测试端口转发逻辑（使用 mock SSH）

### 集成测试

- **MySQL**：使用 Docker 容器启动 MySQL 服务进行测试
- **Redis**：使用 Docker 容器启动 Redis 服务进行测试
- **SSH**：使用本地 SSH 服务器或 Docker 容器测试
- **SSH 隧道**：测试通过跳板机连接的完整流程

### MCP 协议测试

- 测试 JSON-RPC 请求/响应格式
- 测试工具调用和结果返回
- 测试错误场景的协议响应
- 测试并发请求处理

### 测试覆盖目标

- 单元测试覆盖率 > 80%
- 集成测试覆盖所有主要场景
- 错误路径测试覆盖常见失败场景

## 依赖项

### 核心依赖

- `clap` - 命令行参数解析
- `tokio` - 异步运行时
- `anyhow` - 错误处理
- `serde` / `serde_json` - 序列化/反序列化
- `toml` - TOML 配置文件解析
- `serde_yaml` - YAML 配置文件解析

### 连接库

- `mysql_async` - MySQL 客户端
- `redis` - Redis 客户端
- `ssh2` - SSH 客户端

### MCP 相关

- `mcp-sdk` - MCP 协议实现（如果有现成的）
- 或自行实现 JSON-RPC over stdio

### 开发依赖

- `mockall` - Mock 测试
- `testcontainers` - Docker 容器测试

## 实现优先级

### Phase 1: 基础功能

1. 项目结构搭建
2. Config 模块实现（TOML + YAML 支持）
3. MySQL 直接连接（无 SSH 隧道）
4. CLI 模式基本输出

### Phase 2: SSH 支持

1. SSH 直接连接
2. SSH 隧道管理器
3. MySQL 通过 SSH 隧道连接

### Phase 3: 完整功能

1. Redis 支持（直接连接 + SSH 隧道）
2. 多级 SSH 跳板机支持
3. 完善错误处理

### Phase 4: MCP 集成

1. MCP 服务器实现
2. 工具定义和路由
3. JSON-RPC 协议处理

### Phase 5: 测试和优化

1. 单元测试
2. 集成测试
3. 性能优化
4. 文档完善

## 安全考虑

1. **密码存储**：配置文件中的密码应该加密存储或使用密钥管理服务
2. **密钥权限**：检查 SSH 私钥文件权限（应为 600）
3. **连接超时**：设置合理的连接和执行超时，防止资源耗尽
4. **输入验证**：验证所有用户输入，防止注入攻击
5. **日志脱敏**：日志中不记录密码等敏感信息

## 未来扩展

1. **更多服务支持**：PostgreSQL、MongoDB、Kafka 等
2. **连接池**：复用连接提高性能（需要改为守护进程模式）
3. **交互式模式**：支持类似 `mysql -i` 的交互式 shell
4. **批量操作**：支持批量执行多个命令
5. **结果导出**：支持导出为 CSV、JSON 等格式
6. **审计日志**：记录所有操作用于审计
