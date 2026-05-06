# zihuan-next

> 🌐 [English](README.md) | 简体中文

**一个基于 Rust 的节点图工作流平台，用于 AI Agent、同步图执行，以及 QQ Chat Agent、HTTP Stream Agent 等服务化运行时。**

节点图负责描述**类型化数据流**；需要长期运行的 Agent 行为由服务承载，可复用的工作流逻辑则保留在节点和子图中。

<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/3b781e53-1fcf-4b77-91ba-2d63299181c4" />

## 项目概览

`zihuan-next` 现在是一个多 crate Rust workspace，主要有两种使用方式：

- `zihuan_next`：主 Web 应用。负责提供管理界面和图编辑器、暴露 REST/WebSocket API、管理系统配置、执行节点图任务，以及托管长期运行的 Agent。
- `zihuan_graph_cli`：命令行执行器，用于直接从终端运行图 JSON 文件或 `workflow_set/` 中的工作流。

当前架构分层如下：

- `zihuan_graph_engine`：按 DAG 顺序执行**同步**节点图。
- `zihuan_service`：托管 QQ Chat Agent、HTTP Stream Agent 等长期运行服务。
- `zihuan_llm`：提供 LLM、Brain/tool、embedding、检索相关节点与 Agent 辅助逻辑。
- `storage_handler`：提供 MySQL、Redis、RustFS、Weaviate 等连接配置与持久化节点。
- `webui/`：浏览器端 UI，`/` 为 Vue 3 管理界面，`/editor` 为基于 LiteGraph 的节点图编辑器。

## 当前能力

- 浏览器中的可视化节点图编辑器
- 强类型端口的同步 DAG 执行
- 函数子图与 Brain 工具子图
- 带启动、停止、自动启动生命周期的服务化 Agent
- 统一管理连接、LLM 引用和 Agent 配置
- 图任务执行、任务日志和 WebSocket 日志推送
- 本地与远程 embedding 支持
- QQ 消息存储与向量化持久化能力

## 截图

<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/01fae35b-3284-4081-b7f6-f5be5881dc1f" />
<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/d407db1c-2d5c-472e-8689-0ab636dbd7b8" />
<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/40e9d5dc-7383-4f7f-aded-52640edeed8e" />
<img width="1248" height="880" alt="QQ_1774525136280" src="https://github.com/user-attachments/assets/7cc1f27d-9556-4bd7-8741-05904c536490" />
<img width="1248" height="880" alt="6e9a6276770f6a190161b14577ebeb7f" src="https://github.com/user-attachments/assets/6d56ffd6-846f-4ced-9d98-0f57bb8f7d31" />
<img width="2382" height="1647" alt="c5872ca13db7d67512a625e9dae1a601" src="https://github.com/user-attachments/assets/2409f7a6-94a9-46a1-aca8-d21c0fa4347c" />
<img width=600 src="https://github.com/user-attachments/assets/0d25ce93-0f97-4d8c-8375-63b99f6dcd14" />
<img width="1080" src="https://github.com/user-attachments/assets/60b3b145-7ce7-4a76-9742-b975578a9556" />
<img width="1080" src="https://github.com/user-attachments/assets/137e4808-5ce3-4714-a0e3-6f5ddaf9f9cb" />
<img width="1440" src="https://github.com/user-attachments/assets/994472eb-2d37-4160-811d-c5b4856e3239" />
<img width=600 src="https://github.com/user-attachments/assets/12c27199-2b1e-41ab-8215-0baced40dff9" />
<img width=600 src="https://github.com/user-attachments/assets/b30bcef5-cb81-4173-8aa9-cefa5da9e690" />
<img width=600 src="https://github.com/user-attachments/assets/91da8e34-6feb-4c7b-be45-efd8bf599d1f" />

## Workspace 结构

| 包 | 职责 |
|---|---|
| `zihuan_core` | 通用错误类型、系统配置、适配器模型、LLM 模型类型 |
| `zihuan_graph_engine` | 图运行时、节点注册表、图 JSON 加载、基础节点 |
| `zihuan_llm` | LLM 节点、Brain/tool 运行时、embedding、RAG、Agent 配置模型 |
| `storage_handler` | 连接配置，以及 Redis/MySQL/RustFS/Weaviate 节点与辅助逻辑 |
| `ims_bot_adapter` | QQ/IMS 适配器客户端与相关节点 |
| `zihuan_service` | 长期运行 Agent 的服务运行时与定时任务支持 |
| `zihuan_graph_cli` | 终端图执行器 |
| `webui/` | Vue 3 管理界面与 `/editor` 图编辑器 |
| `src/` | 主 Salvo 服务二进制与 HTTP/WebSocket API |

## 快速开始

### 依赖

- Rust stable
- Node.js 18+
- `pnpm`

按需准备的可选服务：

- Redis
- RustFS
- Weaviate
- MySQL

### 构建 Web 应用

```bash
git clone https://github.com/FredYakumo/zihuan-next.git
cd zihuan-next
git submodule update --init --recursive

cd webui
pnpm install
cd ..

cargo build --release
```

`build.rs` 会把 `webui/dist/` 嵌入主二进制，因此 `zihuan_next` 构建前端必须成功。

### 运行 Web 应用

```bash
docker compose -f docker/docker-compose.yaml up -d

./target/release/zihuan_next
# 默认访问 http://127.0.0.1:9951
```

自定义监听地址：

```bash
./target/release/zihuan_next --host 0.0.0.0 --port 9000
```

仓库自带的 compose 会启动：

- Redis：`127.0.0.1:6379`
- RustFS：`127.0.0.1:9000`，控制台 `127.0.0.1:9001`
- Weaviate：`127.0.0.1:8080`

默认**不会**启动 MySQL。

### 构建并运行 CLI

```bash
cargo build -p zihuan_graph_cli --release

./target/release/zihuan_graph_cli --file workflow_set/qq_agent_example.json
./target/release/zihuan_graph_cli --workflow qq_agent_example
```

CLI 会初始化与 Web 图执行一致的节点注册表扩展，然后加载并执行一次节点图。

## 配置模型

当前项目有三层配置：

### 1. 系统配置

由 Web 应用管理，并以 JSON 形式保存在用户配置目录中：

- Windows：`%APPDATA%/zihuan-next_aibot/system_config/system_config.json`
- Linux/macOS：`$XDG_CONFIG_HOME` 或 `$HOME/.config/zihuan-next_aibot/system_config/system_config.json`

当前保存的 section：

- `connections`
- `llm_refs`
- `agents`

### 2. 图定义与图内本地值

图结构、inline values、variables、metadata 和子图都保存在图 JSON 中。工作流集文件默认放在 `workflow_set/`。

### 3. Alembic 迁移配置

`config.yaml` 只给 Python 迁移工具链初始化 MySQL 表结构使用，Rust 运行时**不会**读取 `config.yaml`。

## 可选：本地 Embedding 的 GPU 构建

根 crate 的 feature 会透传给 `zihuan_llm`：

```bash
cargo build --release --features candle-cuda
cargo build --release --features candle-metal
```

Windows 推荐使用辅助脚本：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\cargo-cuda.ps1 build --release
```

本地 embedding 加载器会按 `CUDA -> Metal -> CPU` 顺序尝试，并在 GPU 初始化失败时自动回退。

## 本地 Embedding 模型

本地 embedding 节点从这里加载模型：

```text
models/text_embedding/<model_name>/
```

示例：

```bash
pip install huggingface_hub
hf download Qwen/Qwen3-Embedding-0.6B \
  --local-dir models/text_embedding/Qwen3-Embedding-0.6B
```

目录名必须和图里配置的 `model_name` 一致。

## MySQL 表结构初始化

只有在使用 MySQL 消息存储时才需要。

```bash
cp config.yaml.example config.yaml
uv sync
uv run alembic upgrade head
```

所需的 `MYSQL_*` 字段已在 `config.yaml.example` 中说明。

## 文档

- [用户指南](document/user-guide.zh-CN.md)
- [程序执行流程](document/program-execute-flow.zh-CN.md)
- [开发文档索引](document/dev-guides/README.md)
- [UI 架构](document/dev-guides/ui-architecture.zh-CN.md)
- [节点生命周期](document/node/node-lifecycle.zh-CN.md)
- [函数子图](document/node/function-subgraphs.zh-CN.md)
- [节点开发指南](document/node/node-development.zh-CN.md)

## 许可证

AGPL-3.0，详见 [LICENSE](LICENSE)。
