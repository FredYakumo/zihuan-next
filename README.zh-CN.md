# zihuan-next

> [English](README.md) | 简体中文

`zihuan-next` 是一个基于 Rust 的 Agent 服务平台，围绕两个核心展开：

- Agent 以持久服务的方式运行
- 节点图负责定义可复用工作流

节点图专注于数据流和步骤编排；聊天 Agent、HTTP Agent、连接复用、任务托管、运行时编排等长期运行能力由服务层承载。

<img width="1248" height="880" alt="zihuan-next" src="https://github.com/user-attachments/assets/3b781e53-1fcf-4b77-91ba-2d63299181c4" />

## 快速开始

### 依赖

- Rust stable
- Node.js 18+
- `pnpm`

按需准备的可选服务：

- MySQL
- Redis
- Weaviate
- RustFS

### 构建

```bash
git clone https://github.com/FredYakumo/zihuan-next.git
cd zihuan-next
git submodule update --init --recursive

cd webui
pnpm install
cd ..

cargo build --release
```

主程序会把 `webui/dist/` 前端产物嵌入二进制。

### 运行

```bash
docker compose -f docker/docker-compose.yaml up -d
./target/release/zihuan_next
```

默认地址：

```text
http://127.0.0.1:9951
```

自定义监听：

```bash
./target/release/zihuan_next --host 0.0.0.0 --port 9000
```

### 首次使用顺序

启动后，通常按这个顺序使用：

1. 打开 `/`，先创建 `connections`。
2. 创建 `llm_refs`。
3. 在 `/editor` 中创建或导入工作流图。
4. 把图工作流挂载为 Agent 的工具。
5. 启动 Agent，或直接把图作为任务执行。

## 特点

- 简单的 Agent 能力开箱可用。
- 更复杂的流程可以交给节点图来设计、编排和复用。
- 同一套工作流既可以直接作为任务执行，也可以挂载成 Agent 工具。
- 连接配置和模型引用统一管理一次，随后可在 Agent 和节点图之间复用。

### CLI 图执行器

```bash
cargo build -p zihuan_graph_cli --release

./target/release/zihuan_graph_cli --file workflow_set/qq_agent_example.json
./target/release/zihuan_graph_cli --workflow qq_agent_example
```

## 如何使用

### 1. 先配置共享资源

在管理界面中创建：

- `connections`
- `llm_refs`
- `agents`

这些内容会统一保存在系统配置中心里。

### 2. 再构建工作流图

在 `/editor` 中定义：

- 工作流步骤
- 节点参数
- 函数子图
- 工具子图
- 图内变量和 inline values

### 3. 把工作流挂到 Agent 上

把图工作流作为 Agent tool 挂载到 Agent 定义里，让 Agent 在推理过程中按需调用。简单的 Agent 行为可以保持轻量，而更复杂的多步骤逻辑则适合沉淀为可复用的图工作流。

### 4. 运维运行时

在管理界面中可以：

- 查看任务
- 观察日志
- 管理保存的连接配置
- 查看运行时连接实例
- 启停 Agent

## 截图

<img width="1248" height="880" alt="main-ui" src="https://github.com/user-attachments/assets/01fae35b-3284-4081-b7f6-f5be5881dc1f" />
<img width="1248" height="880" alt="graph-editor" src="https://github.com/user-attachments/assets/d407db1c-2d5c-472e-8689-0ab636dbd7b8" />
<img width="1248" height="880" alt="workflow" src="https://github.com/user-attachments/assets/40e9d5dc-7383-4f7f-aded-52640edeed8e" />
<img width="1248" height="880" alt="qq" src="https://github.com/user-attachments/assets/7cc1f27d-9556-4bd7-8741-05904c536490" />
<img width="1248" height="880" alt="agent" src="https://github.com/user-attachments/assets/6d56ffd6-846f-4ced-9d98-0f57bb8f7d31" />
<img width="2382" height="1647" alt="editor-large" src="https://github.com/user-attachments/assets/2409f7a6-94a9-46a1-aca8-d21c0fa4347c" />
<img width="600" alt="shot-1" src="https://github.com/user-attachments/assets/0d25ce93-0f97-4d8c-8375-63b99f6dcd14" />
<img width="1080" alt="shot-2" src="https://github.com/user-attachments/assets/60b3b145-7ce7-4a76-9742-b975578a9556" />
<img width="1080" alt="shot-3" src="https://github.com/user-attachments/assets/137e4808-5ce3-4714-a0e3-6f5ddaf9f9cb" />
<img width="1440" alt="shot-4" src="https://github.com/user-attachments/assets/994472eb-2d37-4160-811d-c5b4856e3239" />
<img width="600" alt="shot-5" src="https://github.com/user-attachments/assets/12c27199-2b1e-41ab-8215-0baced40dff9" />
<img width="600" alt="shot-6" src="https://github.com/user-attachments/assets/b30bcef5-cb81-4173-8aa9-cefa5da9e690" />
<img width="600" alt="shot-7" src="https://github.com/user-attachments/assets/91da8e34-6feb-4c7b-be45-efd8bf599d1f" />

## 项目是什么

`zihuan-next` 把下面几部分组合在一起：

- 持久运行的 Agent Runtime
- 浏览器中的工作流编辑器
- 同步 DAG 节点图执行引擎
- Agent 与图工具共用的 tool-call loop
- 统一的连接、模型引用、Agent 配置中心

实际使用时，它主要有三种连在一起的方式：

1. 把 Agent 作为常驻服务运行。
2. 用节点图编辑器编排工作流。
3. 把这些工作流挂载成 Agent 可调用的工具。

这样可以让图结构始终保持简单，把复杂性收敛到节点、子图和 Agent 的工具调用循环里。

## 核心模型

### 1. Agent service 是主运行时

主程序会托管长期运行的 Agent，例如：

- `qq_chat`
- `http_stream`

这些 Agent 可以在管理界面中启用、禁用、启动、停止和自动启动。它们不是一次性脚本，而是由服务运行时持续托管的能力。

### 2. 节点图是工作流

图执行引擎按 DAG 拓扑同步运行，适合用来描述：

- 数据转换
- 消息处理
- 检索与存储步骤
- 模型调用
- 工具结果整理
- 封装成可复用子图的业务逻辑

长期监听、服务生命周期、后台常驻行为不放在图里，而是交给服务层。

### 3. 工作流也可以成为 Agent 工具

这是 `zihuan-next` 最重要的设计点之一。

同一套节点图逻辑可以同时承担两种角色：

- 直接作为工作流执行
- 作为 Agent 的可调用工具挂载进去

Agent 通过共享的 Brain/tool loop 调用这些图工具。这样一套工作流逻辑可以在交互式 Agent、服务接口和图执行任务之间复用，而不需要重复实现。

## 统一的连接与资源配置

连接是系统级的一等配置，不是某个工作流里临时写死的参数。

你可以在管理界面中定义一次连接配置，然后同时给下面两类能力复用：

- Agent
- 节点图

当前项目中的资源类型包括：

- MySQL
- Redis
- Weaviate
- RustFS / S3 风格对象存储
- IMS Bot Adapter 连接
- Tavily

运行时会区分两层：

- 持久保存的连接配置，使用 `config_id`
- 实际创建出来的运行时连接实例，使用 `instance_id`

节点图和 Agent 都只引用 `config_id`。真正的 live 连接由运行时按需创建和复用。这样数据库和外部服务连接可以统一管理，同时又能被图节点和 Agent 直接使用。

## 模型接入

`zihuan-next` 支持多种 LLM 与 embedding 使用方式：

- 基于 Candle 的本地推理
- 基于 `llama.cpp` 的本地或自托管推理
- 在线模型 API
- 兼容 OpenAI Chat Completions 的接口
- 兼容 OpenAI Responses 的接口

模型端点通过系统配置中的 `llm_refs` 统一定义，再按需绑定到 Agent 或节点图。

这使得同一套部署可以灵活混合：

- 为成本或隐私使用本地推理
- 为内网服务使用自托管推理
- 为通用能力接入托管 API

## 主要能力

- `/` 提供浏览器管理界面
- `/editor` 提供浏览器节点图编辑器
- 持久 Agent 托管
- 节点图任务执行
- 图工作流作为 Agent 工具复用
- 共享 Brain 工具调用循环
- 统一连接与模型配置
- REST API 与 WebSocket 事件流
- 任务日志与运行时状态查看
- `workflow_set` 工作流加载与 CLI 执行

## Workspace 结构

| 包 | 职责 |
|---|---|
| `zihuan_core` | 通用类型、配置、错误定义 |
| `zihuan_agent` | Brain 工具调用循环引擎 |
| `zihuan_graph_engine` | 同步 DAG 图执行运行时 |
| `model_inference` | LLM、embedding 与推理相关节点 |
| `storage_handler` | 各类连接节点与运行时连接管理 |
| `ims_bot_adapter` | IMS / QQ 适配层 |
| `zihuan_service` | 长期运行 Agent 托管与 Agent 节点 |
| `zihuan_graph_cli` | CLI 图执行器 |
| `webui/` | Vue 管理界面与 LiteGraph 编辑器 |
| `src/` | 主 Salvo Web 服务、API 与应用运行时 |

## 配置模型

系统级配置默认保存于：

- Windows：`%APPDATA%/zihuan-next_aibot/system_config/system_config.json`
- Linux/macOS：`$XDG_CONFIG_HOME` 或 `$HOME/.config/zihuan-next_aibot/system_config/system_config.json`

当前结构：

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

图结构、inline values、variables 与嵌入式子图保存在图 JSON 或 `workflow_set/` 下的工作流文件中。

`config.yaml` 只用于 Python Alembic 的 MySQL 表结构迁移流程。

## 文档

- [用户指南](document/user-guide.zh-CN.md)
- [程序执行流程](document/program-execute-flow.zh-CN.md)
- [配置与连接实例](document/config-and-connection-instances.zh-CN.md)
- [节点系统](document/dev-guides/node-system.zh-CN.md)
- [代码规范](document/dev-guides/code-conventions.zh-CN.md)
- [UI 架构](document/dev-guides/ui-architecture.zh-CN.md)
- [函数子图](document/node/function-subgraphs.zh-CN.md)
- [节点开发指南](document/node/node-development.zh-CN.md)

## 许可证

AGPL-3.0，详见 [LICENSE](LICENSE)。
