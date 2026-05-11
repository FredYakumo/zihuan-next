# UI 架构

本文档描述项目当前的浏览器端 UI 架构。

## 路由拆分

主二进制当前提供一套前端资源，但有两个入口体验：

- `/` -> Vue 3 管理界面
- `/editor` -> 浏览器节点图编辑器

入口切换逻辑位于 `webui/src/main.ts`。

## 前端技术栈

- Vite
- TypeScript
- Vue 3 负责管理界面
- LiteGraph.js 负责节点图画布

当前架构中不存在桌面 GUI 层。

## 前端目录结构

| 路径 | 职责 |
|---|---|
| `webui/src/main.ts` | 前端启动与入口分流 |
| `webui/src/admin/` | Vue 管理界面与各视图 |
| `webui/src/graph/` | LiteGraph 集成、画布、组件、注册表、历史记录 |
| `webui/src/app/` | 编辑器侧状态与任务/工作区辅助逻辑 |
| `webui/src/api/` | 浏览器侧 HTTP/WebSocket 客户端封装 |
| `webui/src/ui/` | 通用 shell、对话框与主题支持 |

## 管理界面

当路径不是 `/editor` 时，会挂载 Vue 管理端。

当前顶层路由：

- `/`
- `/connections`
- `/llm`
- `/agents`
- `/graphs`
- `/tasks`

管理界面负责：

- 编辑系统配置记录
- 浏览图与任务
- 展示运行时/任务状态
- 主题切换

## 图编辑器

当路径以 `/editor` 开头时，会调用 `bootstrapGraphEditor()` 启动浏览器图编辑器。

图编辑器负责：

- 画布渲染
- 节点面板与连线编辑
- inline widget 渲染
- 图保存/加载交互
- 通过后端 API 发起验证与执行

图编辑器不会把图工作区或标签页状态持久化到浏览器存储。重新加载 `/editor` 时会从空工作区开始，除非用户显式打开某个文件，或使用了指向某个工作流的路由。

执行语义、持久化状态和节点注册表元数据仍由后端掌控。

## 与后端的边界

浏览器前端通过以下方式与 Rust 后端通信：

- `/api` 下的 REST 接口
- `/api/ws` 的 WebSocket 推送

后端负责：

- 图 session 与执行
- task 记录
- 系统配置持久化
- 节点注册表元数据
- 日志转发
- 服务化 Agent

前端负责：

- 展示层
- 本地交互状态
- 路由导航
- 画布交互状态

这些本地交互状态只在内存中按会话维持。图内容的持久化依赖显式保存流程，或后端管理的图/session API，而不是浏览器 `localStorage`。

## 主题系统

主题逻辑位于 `webui/src/ui/theme.ts`。

浏览器 UI 可以加载内置主题和自定义主题，并将其应用到：

- 管理界面的 CSS 变量
- LiteGraph 的视觉 token

后端通过 `/api/themes` 提供主题相关接口。

## WebSocket 事件用途

浏览器通过 `/api/ws` 接收：

- task 生命周期事件
- 实时日志
- 图验证结果
- 节点 QQ 消息预览数据

这样长时间运行的反馈就不必依赖轮询。

对于任务 UI，需要注意当前语义：

- graph task 表示一次图执行
- agent task 表示一次已处理的响应/请求
- 启动 agent 不会创建 task 行
- QQ agent task 的名称通常类似 `回复[sender_id]的消息`
- agent task 日志来自已持久化的 task 日志文件，而不仅是实时 WebSocket 输出

## 编辑器相关补充

图编辑器仍然使用基于 LiteGraph 的浏览器画布。如果你要修改 inline widget、画布渲染或连线布局，建议同时查看：

- `webui/src/graph/canvas.ts`
- `webui/src/graph/widgets.ts`
- `webui/src/graph/inline_layout.ts`
- `webui/src/graph/link_layout.ts`

核心原则不变：浏览器负责渲染和编辑，执行语义归 Rust 后端所有。
