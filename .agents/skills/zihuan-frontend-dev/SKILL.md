---
name: zihuan-frontend-dev
description: Develop the zihuan-next WebUI. Use this skill when asked to modify the admin panel (Vue 3), the visual node-graph editor (Litegraph.js), add UI features, or change the TypeScript frontend code in webui/.
---

# Frontend Development in zihuan-next

The WebUI has two subsystems served from the same Vite + TypeScript application:

| Subsystem | Route | Framework | Purpose |
|-----------|-------|-----------|---------|
| **Admin UI** | `/` | Vue 3 + Vue Router | Manage connections, agents, LLMs, graphs, tasks, commands, settings |
| **Graph Editor** | `/editor` | Litegraph.js (vanilla TS) | Visual node-graph canvas for workflow editing |

The routing split happens in `webui/src/main.ts`: `window.location.pathname.startsWith("/editor")` forks to the graph editor bootstrap; all other paths render the Vue admin app.

## Tech stack

| Layer | Technology |
|-------|-----------|
| Build tool | Vite 5 |
| Language | TypeScript (strict) |
| Admin framework | Vue 3 + Vue Router 4 |
| Admin styling | Sass (SCSS) |
| Graph canvas | Litegraph.js 0.7 |
| State / communication | REST (fetch) + WebSocket (real-time) |
| Package manager | pnpm 10 |

## Directory structure

```
webui/src/
├── main.ts                    # Entry: route dispatch (admin vs editor)
├── graph_editor_bootstrap.ts  # Litegraph.js editor init
├── api/                       # Backend communication
│   ├── client.ts              # REST API client
│   ├── logger.ts              # Log streaming
│   ├── types.ts               # API type definitions
│   └── ws.ts                  # WebSocket client
├── admin/                     # Vue 3 Admin UI
│   ├── AdminApp.vue           # Shell: sidebar + <RouterView>
│   ├── admin.scss             # Admin global styles
│   ├── model.ts               # Shared TS types (ConnectionType, AgentTypeName, etc.)
│   ├── styles/                # Shared SCSS variables / mixins
│   └── view/                  # One Vue component per page
│       ├── Dashboard.vue      # Agent runtime overview + inline chat test
│       ├── Connections.vue    # Connection config CRUD (MySQL, Redis, Weaviate, etc.)
│       ├── ConnectionManager.vue  # Active connection pool status
│       ├── Llm.vue            # LLM service & model config
│       ├── Agents.vue         # Agent CRUD (QQ Chat / HTTP Stream)
│       ├── Graphs.vue         # Graph & workflow management
│       ├── Tasks.vue          # Task history & monitoring
│       ├── Commands.vue       # Command permission management
│       ├── DataExplorer.vue   # Message / data retrieval
│       └── Settings.vue       # Global settings
├── app/                       # Graph editor application logic
│   ├── lifecycle.ts, tab_manager.ts, save_manager.ts, shortcuts.ts, workspace.ts
├── graph/                     # Litegraph.js visualization
│   ├── canvas.ts, history.ts, registry.ts, inline_layout.ts, link_layout.ts, widgets/
├── ui/                        # Shared UI utilities
│   ├── theme.css, theme.ts    # Theme system
│   └── live_log_console.ts    # Floating log console
└── assets/                    # Brand images, icons
```

## Admin UI architecture (Vue 3)

### Routing (defined in `main.ts`)

| Path | Component | Purpose |
|------|-----------|---------|
| `/` | `Dashboard.vue` | Runtime overview, agent chat test |
| `/connections` | `Connections.vue` | Configure MySQL, Redis, Weaviate, S3, bot adapters, tokenizer |
| `/connection-manager` | `ConnectionManager.vue` | Active connection pool monitoring |
| `/llm` | `Llm.vue` | LLM service & model reference management |
| `/agents` | `Agents.vue` | Create/edit QQ Chat and HTTP Stream agents |
| `/graphs` | `Graphs.vue` | Manage node graphs and workflow definitions |
| `/tasks` | `Tasks.vue` | Task execution history |
| `/commands` | `Commands.vue` | Command permission CRUD |
| `/data-explorer` | `DataExplorer.vue` | Browse messages, media, and data stores |
| `/settings` | `Settings.vue` | Global configuration |

### Key types (from `admin/model.ts`)

```typescript
// Connection types
export type ConnectionType = "mysql" | "redis" | "weaviate" | "rustfs"
  | "bot_adapter" | "ims_bot_adapter" | "web_search_engine" | "tokenizer" | "sqlite";

// Agent types
export type AgentTypeName = "qq_chat" | "http_stream";

// LLM API styles
export type LlmApiStyle = "candle" | "open_ai_chat_completions"
  | "open_ai_chat_completions_tencent_multimodal_compat" | "open_ai_responses"
  | "open_ai_responses_message_compat" | "open_ai_responses_image_url_object_compat";

// Tool definition types
export type ToolTargetType = "workflow_set" | "file_path" | "inline_graph";
export type ToolRunDuration = "Short" | "Long";
```

## Running the dev server

```bash
cd webui
pnpm install
pnpm run dev     # HMR dev server on localhost:5173
```

The Vite dev server proxies API requests to the Rust backend (default `localhost:9951`). Configure the proxy target in `vite.config.ts` if your backend is on a different port.

## Building for production

```bash
cd webui
pnpm run build   # tsc --noEmit + vite build → webui/dist/
```

This is also triggered automatically by the Rust `build.rs` script when building the full project.

## Agent tips

- **Two separate UIs** — the admin panel (`/`) and graph editor (`/editor`) are completely independent code paths with separate entry points. Changes to one don't affect the other.
- **Use `pnpm dev` for frontend-only iteration** — much faster than full Rust rebuilds, with Vue HMR.
- **Admin model types live in `admin/model.ts`** — add new shared types here rather than scattering them across view components.
- **Admin views are single-file components** — each `.vue` file is self-contained with `<template>`, `<script setup>`, and `<style scoped>`.
- **WebSocket handles real-time updates** — task progress, log streaming, and execution status flow through `webui/src/api/ws.ts`. Init in `main.ts` via `ws.connect()`.
- **TypeScript is strict mode** — ensure all types are properly defined before using them.
- **Sass is available globally** — `admin/admin.scss` and `admin/styles/` provide shared variables and mixins for admin views.
- **The Litegraph.js editor is vanilla TypeScript** — no Vue reactivity, direct DOM manipulation for the graph canvas.
