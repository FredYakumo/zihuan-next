# 用户指南

本指南描述的是项目**当前**的使用方式：一个 Web 应用，加上一个可选的 CLI 图执行器。

## 实际要运行什么

当前对用户可见的二进制有两个：

- `zihuan_next`：主 Web 应用
- `zihuan_graph_cli`：终端图执行器

大多数情况下，先使用 `zihuan_next`。

## 1. 构建项目

依赖：

- Rust stable
- Node.js 18+
- `pnpm`

构建步骤：

```bash
git clone <repository-url>
cd zihuan-next
git submodule update --init --recursive

cd webui
pnpm install
cd ..

cargo build --release
```

主二进制会嵌入 `webui/dist/` 中构建好的前端资源。

## 2. 启动配套服务

仓库自带的 Docker Compose 会启动很多图和 Agent 常用的本地依赖：

```bash
docker compose -f docker/docker-compose.yaml up -d
```

包含的服务：

- Redis
- RustFS
- Weaviate

其中不包含 MySQL。如果你需要 MySQL 消息存储，请单独启动 MySQL。

## 3. 运行 Web 应用

默认启动：

```bash
./target/release/zihuan_next
```

默认监听地址：

```text
127.0.0.1:9951
```

自定义 host/port：

```bash
./target/release/zihuan_next --host 0.0.0.0 --port 9000
```

也可以用环境变量：

- `ZIHUAN_HOST`
- `ZIHUAN_PORT`

## 4. 打开界面

Web 应用当前提供两个浏览器入口：

- `/` -> Vue 3 管理界面
- `/editor` -> 节点图编辑器

管理界面用于：

- 管理连接配置
- 管理 LLM refs
- 管理 agents
- 管理已保存的图 session
- 查看任务与日志

图编辑器用于构建和运行节点图。

## 5. 理解当前配置模型

### 系统配置

系统级 JSON 配置保存在：

- Windows：`%APPDATA%/zihuan-next_aibot/system_config/system_config.json`
- Linux/macOS：`$XDG_CONFIG_HOME` 或 `$HOME/.config/zihuan-next_aibot/system_config/system_config.json`

当前磁盘结构：

```json
{
  "version": 2,
  "configs": {
    "connections": [],
    "llm_refs": [],
    "agents": []
  }
}
```

其中核心配置集合仍然是：

- `connections`
- `llm_refs`
- `agents`

实际主键统一为：

- `config_id`

旧版顶层 `connections` / `llm_refs` / `agents` 会在读取时自动迁移。

### 图文件

图结构、inline values、variables、metadata 和嵌入式子图都保存在图 JSON 文件里。

工作流集文件默认位于：

```text
workflow_set/
```

### `config.yaml`

`config.yaml` 只给 Python Alembic 迁移使用，Rust 运行时不会读取它。

## 6. 配置连接与 Agent

在管理界面中可以：

1. 创建 Redis、MySQL、RustFS、Weaviate、Tavily、bot adapter 等连接记录。
2. 创建可复用的 LLM ref。
3. 创建需要长期运行的 Agent，例如 QQ Chat Agent 或 HTTP Stream Agent。

当前管理界面中：

- 连接配置、模型配置、Agent 配置都以 `config_id` 作为主键
- 卡片里的长 ID 会缩短显示，例如 `abcd1234...`
- 如果一个连接配置当前对应多个运行时实例，相关页面会显示成 `abcd1234..., 等N个`

如果某个 Agent 同时设置了 `enabled = true` 和 `auto_start = true`，那么 `zihuan_next` 启动时会自动拉起它。

如果你需要区分“保存下来的连接定义”和“运行中的 live 连接实例”，请继续阅读：

- [配置与连接实例](./config-and-connection-instances.zh-CN.md)

## 7. 使用节点图

你可以：

- 在 `/editor` 中创建和编辑图
- 打开和保存 `workflow_set/` 工作流
- 从 Web UI 直接执行图
- 在任务列表里重新执行带文件路径的历史任务

通过 Web 应用执行图时，会创建 task 记录，并通过 WebSocket 推送日志和运行事件。

## 8. 用 CLI 执行图

先构建 CLI：

```bash
cargo build -p zihuan_graph_cli --release
```

按文件路径执行：

```bash
./target/release/zihuan_graph_cli --file workflow_set/qq_agent_example.json
```

按工作流名称执行：

```bash
./target/release/zihuan_graph_cli --workflow qq_agent_example
```

CLI 会加载图、构建 `NodeGraph`、执行一次并退出。

## 9. 可选：初始化 MySQL 表结构

只有在使用 MySQL 消息存储时才需要：

```bash
cp config.yaml.example config.yaml
uv sync
uv run alembic upgrade head
```

迁移连接会根据 `config.yaml` 中的 `MYSQL_*` 字段生成。

## 10. 可选：为本地 Embedding 启用 GPU 构建

CUDA：

```bash
cargo build --release --features candle-cuda
```

Metal：

```bash
cargo build --release --features candle-metal
```

Windows 推荐脚本：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\cargo-cuda.ps1 -Release build
```

运行时本地 embedding 加载器会按 `CUDA -> Metal -> CPU` 顺序尝试。
