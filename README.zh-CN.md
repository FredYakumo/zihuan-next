# zihuan-next

> 🌐 [English](README.md) | 简体中文

**一个 AI 智能体运行引擎，用节点图描述简单或事件驱动的 Agentic 工作流，只需几步即可运行 AI 智能体，并可以轻松且准确地定义数据流程，可以清晰地看到数据如何在流水线中流动。**

内置一批开箱即用的 AI 智能体模板。

<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/3b781e53-1fcf-4b77-91ba-2d63299181c4" />

## 概述

zihuan-next 使用**节点图**描述数据在工作流中的流动方式。你可以把它理解为一个由类型化处理块构成的简单流程图：数据进来，经过几个步骤，产生输出。

> **图描述数据流向何处，复杂性封装在各个节点内部。**

一个工作流可能如下所示：

`接收消息 → 提取文本 → 调用模型 → 格式化回复 → 发送消息`

每个节点都有清晰的**输入**和**输出**，并带有声明的数据类型。图聚焦于**全局视图**：数据从哪里进入、如何转换、流向何处。Agentic 行为（LLM 循环、工具调用、检索）封装在专用节点内部——不以图拓扑的形式呈现。当某个问题过于复杂、无法在画布上清晰表达时，创建新节点或函数子图，而不是让主图变得更复杂。

---

编辑器运行在浏览器中。后端是一个独立的 Rust 二进制文件（[Salvo](https://salvo.rs)），负责提供 Web UI 并暴露 REST + WebSocket API。前端是基于 [Vite](https://vitejs.dev) 和 [Litegraph.js](https://github.com/Comfy-Org/litegraph.js) 构建的 TypeScript SPA。

### 核心能力

1. **数据流节点图** — 描述数据在各步骤间的流动；内部算法保留在节点内部。
2. **强类型端口** — 每个端口都有声明的类型，使每条连接具有明确的契约。
3. **单二进制** — 只需要一个二进制文件即可运行。
4. **函数子图与智能体工具** — `function` 节点可将私有子图打包为一个可复用的步骤，并将其作为 LLM 驱动节点的可调用工具暴露出去。
5. **可扩展性** — 当工作流需要新行为时，添加新节点；图保持简洁。

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

---

## 快速开始

### 方式 A：下载预构建二进制

从 [Releases 页面](https://github.com/FredYakumo/zihuan-next/releases)（*即将发布*）下载最新发布包，解压后直接运行可执行文件——无需任何构建工具。

### 方式 B：从源码构建

依赖：**Rust**（stable）、**Node.js 18+** 和 **pnpm**。

```bash
# 1. 克隆仓库
git clone https://github.com/FredYakumo/zihuan-next.git
cd zihuan-next

# 2. 安装前端依赖（仅首次）
cd webui && pnpm install && cd ..

# 3. 构建 — build.rs 会自动执行 pnpm build
cargo build --release
```

### 运行

```bash
docker compose -f docker/docker-compose.yaml up -d   # Redis（+ 可选 MySQL）

./target/release/zihuan_next                         # http://127.0.0.1:8080
./target/release/zihuan_next --host 0.0.0.0 --port 9000
```

配置值（API 密钥等）通过节点图内的**超参数**管理——在编辑器的超参数面板中填写即可。

### 构建命令

```bash
cargo build
cargo build --release

# 仅构建前端
cd webui && pnpm run build

# 运行测试
cargo test
```

---

## 文档

- **[用户指南](document/user-guide.zh-CN.md)** — 配置与运行应用
- **[程序执行流程](document/program-execute-flow.zh-CN.md)** — 后端启动与请求生命周期
- **[UI 架构](document/dev-guides/ui-architecture.zh-CN.md)** — Web 前端与 API 设计
- **[节点图 JSON 规范](document/node/node-graph-json.zh-CN.md)** — 节点图的 JSON 保存/加载格式
- **[节点生命周期与执行](document/node/node-lifecycle.zh-CN.md)** — 节点执行模型、调度与数据流
- **[函数子图](document/node/function-subgraphs.zh-CN.md)** — 嵌入式函数子图与可复用子流水线
- **[节点开发指南](document/node/node-development.zh-CN.md)** — 创建自定义节点并扩展系统

## 许可证

AGPL-3.0 — 详见 [LICENSE](LICENSE)。
