# User Guide

This guide explains how to get the application running on your system.

---

## Table of Contents

- [User Guide](#user-guide)
  - [Table of Contents](#table-of-contents)
  - [Installation](#installation)
    - [Option A: Use Pre-built Binaries](#option-a-use-pre-built-binaries)
    - [Option B: Build from Source](#option-b-build-from-source)
  - [Configuration](#configuration)
    - [Prerequisites](#prerequisites)
    - [Hyperparameters](#hyperparameters)
  - [Running the Application](#running-the-application)
    - [Method 1: GUI Mode (Visual Editor)](#method-1-gui-mode-visual-editor)
      - [编辑器菜单与快捷键](#编辑器菜单与快捷键)
      - [保存行为说明](#保存行为说明)
      - [工作流集标记](#工作流集标记)
    - [Method 2: Headless Mode (CLI / Production)](#method-2-headless-mode-cli--production)
    - [Method 3: Validate Mode (Pre-flight Check)](#method-3-validate-mode-pre-flight-check)

---

## Installation

### Option A: Use Pre-built Binaries

1.  Download the latest release package from the repository's Releases page.
2.  Extract the archive to a folder of your choice.
3.  Ensure the folder contains the executable and the configuration files.
4.  Start the application:
    - **Windows:** Double-click `zihuan_next.exe` or run in terminal:
      ```powershell
      .\zihuan_next.exe
      ```
    - **Linux:** In terminal, run:
      ```bash
      ./zihuan_next
      ```
    - **macOS:** In terminal, run:
      ```bash
      ./zihuan_next
      ```




### Option B: Build from Source

If you are a developer or want the latest changes:

1.  **Install Rust:** Ensure you have the Rust toolchain installed (1.70+).
2.  **Clone the repository:**
    ```bash
    git clone <repository-url>
    cd zihuan-next_aibot-800b
    ```
3.  **Build the release binary:**
    ```bash
    cargo build --release
    ```
    The executable will be located in `./target/release/`.
    *   Windows: `zihuan_next.exe`
    *   Linux/macOS: `zihuan_next`

---

## Configuration

### Prerequisites

Before running, ensure optional dependencies are ready if you need them:

1.  **Redis**: For message caching (recommended for performance).
2.  **MySQL**: For long-term message persistence.
    ```bash
    # Start Redis and RustFS using Docker (MySQL optional)
    docker compose -f docker/docker-compose.yaml up -d
    
    # Initialize database schema (if using MySQL)
    alembic upgrade head
    ```

### Hyperparameters

Configuration values such as API keys, and database connection strings are managed as **hyperparameters** inside the node graph, not through a `config.yaml` file.

To set hyperparameter values:
1. Open the node graph in the editor.
2. Click the **Hyperparameters** panel (toolbar on the right side).
3. Fill in the required values (e.g., API keys, endpoint URLs).

Values are stored locally and reused across runs by `(group, name)` — renaming or moving the graph file does not break them. Sensitive values such as passwords are masked in the UI.

---

## Running the Application

### Method 1: GUI Mode (Visual Editor)

**Use this mode to create, edit, and test node graphs visually.**

**How to run:**
- **Windows:** Double-click `zihuan_next_aibot-800b.exe`.
- **Command Line:**
    ```bash
    ./zihuan_next_aibot-800b
    ```

**What happens:**
1.  A window opens displaying the node graph editor.
2.  You can drag nodes from the palette, connect them, and verify logic.
3.  Use "Save Graph" to export your workflow to a JSON file (e.g., `bot.json`).

#### 编辑器菜单与快捷键

点击左上角 **Zihuan Next** 标题可打开菜单，所有主要操作均支持键盘快捷键：

| 操作 | 菜单项 | 快捷键 |
|---|---|---|
| 新建节点图 | 新建 | `Ctrl+N` |
| 从本地文件打开 | 打开... | `Ctrl+O` |
| 保存 | 保存 | `Ctrl+S` |
| 另存为 | 另存为 | `Ctrl+Shift+S` |
| 保存为工作流集 | 保存为工作流集 | — |
| 验证图结构 | 验证 | — |

> **注意：** 在输入框或文本域中聚焦时，快捷键不会触发，避免与节点参数编辑冲突。

#### 保存行为说明

**保存（`Ctrl+S`）的逻辑取决于当前标签页的来源：**

- **从服务端工作流集打开的图**（标签页标题显示 `[工作流集]`）：直接静默保存回原工作流集文件，无需弹出对话框。
- **从本地文件打开或新建的图**：保存到服务器记录的文件路径；若没有关联路径则触发浏览器下载。

**另存为（`Ctrl+Shift+S`）：** 弹出对话框，让用户选择保存目标：

- **保存到工作流集**：输入文件名后保存到服务器 `workflow_set/` 目录；保存完成后当前标签页标题变为 `xxx [工作流集]`，后续按 `Ctrl+S` 将静默保存回工作流集。
- **下载到本地**：触发浏览器下载 JSON 文件；保存后标签页不再标记为工作流集。

#### 工作流集标记

通过**打开工作流集**菜单（主界面右侧工具栏）或**保存为工作流集**操作打开的节点图，其标签页和浏览器标题栏会显示 `[工作流集]` 后缀，以区分服务器端托管的工作流与本地文件。

### Method 2: Headless Mode (CLI / Production)

**Use this mode to run a saved bot workflow in the background.**

**How to run:**
You must provide the graph file and the `--no-gui` flag via the command line.

**Windows (PowerShell/CMD):**
```powershell
.\zihuan_next_aibot-800b.exe --graph-json bot.json --no-gui
```

**Linux/macOS:**
```bash
./zihuan_next_aibot-800b --graph-json bot.json --no-gui
```

**Common Flags:**
- `--graph-json <path>`: Path to the JSON file defining your graph.
- `--no-gui`: Disables the window interface.
- `--save-graph-json <path>`: (Optional) Save a processed/validated version of the graph on exit.

**Stopping the bot:**
Press `Ctrl+C` in the terminal to gracefully shut down the application and close connections.

### Method 3: Validate Mode (Pre-flight Check)

**Use this mode to verify a graph JSON file before running it in production.**

The validator checks:
- JSON parsing and schema correctness
- All `node_type` values exist in the node registry
- Required ports are present on every node
- Invalid edge references (unknown node IDs or port names)
- Cycle dependencies (which would prevent execution)
- Embedded subgraphs in `function` and `brain` nodes

**Windows (PowerShell/CMD):**
```powershell
.\zihuan_next.exe --graph-json bot.json --validate
```

**Linux/macOS:**
```bash
./zihuan_next --graph-json bot.json --validate
```

**Example output:**
```
验证节点图: bot.json
  ✓ 文件解析成功（5 个节点，4 条连接）
  ⚠ 警告: 节点 "Format String" 的内联值 "old_key" 对应的端口不存在
  ✗ 错误: 节点图存在环路依赖，涉及节点: "Node A", "Node B"

结果: ✗ 1 个错误，1 个警告 — 节点图无法安全运行
```

**Exit codes:**
- `0` — No errors (graph is safe to run; warnings may still appear)
- `1` — One or more errors found (graph will fail at runtime)
- `2` — File could not be loaded or parsed
