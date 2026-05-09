# UI Architecture

This document describes the current browser UI architecture.

## Route Split

The main binary serves one frontend bundle with two entry experiences:

- `/` -> Vue 3 admin application
- `/editor` -> browser-based node graph editor

Route selection happens in `webui/src/main.ts`.

## Frontend Stack

- Vite
- TypeScript
- Vue 3 for the admin UI
- LiteGraph.js for the graph editor canvas

There is no desktop UI layer in the current architecture.

## Frontend Directory Layout

| Path | Responsibility |
|---|---|
| `webui/src/main.ts` | frontend bootstrap and route split |
| `webui/src/admin/` | Vue admin shell and views |
| `webui/src/graph/` | LiteGraph integration, canvas, widgets, registry, history |
| `webui/src/app/` | editor-side state and task/workspace helpers |
| `webui/src/api/` | browser HTTP/WebSocket client helpers |
| `webui/src/ui/` | shared shell, dialogs, and theme support |

## Admin UI

The admin app is mounted when the path does not start with `/editor`.

Current top-level routes:

- `/`
- `/connections`
- `/llm`
- `/agents`
- `/graphs`
- `/tasks`

The admin UI is responsible for:

- editing system config records
- browsing graphs and tasks
- surfacing runtime/task state
- theme selection

## Graph Editor

When the path starts with `/editor`, `bootstrapLegacyEditor()` is used to start the browser graph editor.

The editor is responsible for:

- graph canvas rendering
- node palette and connection editing
- inline widget rendering
- graph save/load interactions
- validation and execution requests through backend APIs

The backend remains the source of truth for execution, saved graph state, and registry metadata.

## Backend Boundary

The browser frontend talks to the Rust backend through:

- REST endpoints under `/api`
- WebSocket updates from `/api/ws`

The backend owns:

- graph sessions and execution
- task records
- system config persistence
- node registry metadata
- log forwarding
- service-hosted agents

The frontend owns:

- presentation
- local interaction state
- route navigation
- canvas interaction state

## Theme System

Theme assets are loaded in the browser via `webui/src/ui/theme.ts`.

The browser UI can load built-in and custom themes and applies them to:

- admin UI CSS variables
- LiteGraph visual tokens

Theme APIs are exposed from the backend under `/api/themes`.

## WebSocket Event Usage

The browser uses `/api/ws` to receive:

- task lifecycle events
- live log messages
- graph validation results
- node preview QQ messages

This keeps long-running feedback out of request/response polling loops.

For the task UI, remember the current semantics:

- graph tasks represent one graph execution
- agent tasks represent one handled response/request
- starting an agent does not create a task row
- QQ agent tasks are typically named like `回复[sender_id]的消息`
- agent task logs come from persisted task log files, not only live WebSocket output

## Editor-Specific Notes

The graph editor still uses the LiteGraph-based browser canvas. If you touch inline widgets, canvas rendering, or link layout, also review:

- `webui/src/graph/canvas.ts`
- `webui/src/graph/widgets.ts`
- `webui/src/graph/inline_layout.ts`
- `webui/src/graph/link_layout.ts`

The important rule is unchanged: browser code renders and edits graphs, but execution semantics belong to Rust.
