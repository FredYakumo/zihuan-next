# 主题编写指南

> 🌐 [English](theme-guide.md) | 简体中文

本指南介绍如何为 Zihuan Next 编写自定义主题。

---

## 目录

- [主题系统概述](#主题系统概述)
- [主题文件位置](#主题文件位置)
- [主题文件格式](#主题文件格式)
- [CSS 变量参考](#css-变量参考)
- [LiteGraph 颜色参考](#litegraph-颜色参考)
- [完整示例](#完整示例)
- [调试技巧](#调试技巧)

---

## 主题系统概述

Zihuan Next 支持通过 JSON 文件自定义编辑器外观。主题分为两部分：

1. **CSS 变量**：控制 DOM 界面（工具栏、对话框、节点面板等）的颜色。
2. **LiteGraph 颜色**：控制画布区域（节点卡片、连线、网格等）的颜色。

系统内置了两套默认主题（`default_dark` 和 `default_light`），无需额外文件即可使用。你可以通过编写主题文件来扩展更多配色方案。

---

## 主题文件位置

在可执行文件的工作目录下创建 `custom_themes/` 文件夹，将主题 JSON 文件放入其中：

```
zihuan_next/
├── custom_themes/
│   ├── my_theme.json
│   └── another_theme.json
├── config.yaml
└── ...
```

启动服务后，前端会自动通过 `/api/themes` 读取该目录下的所有主题文件，并出现在编辑器菜单的**主题**选择窗口中。

---

## 主题文件格式

每个主题是一个 JSON 文件，顶层结构如下：

```json
{
  "name": "my_dark",
  "display_name": "我的暗色主题",
  "mode": "dark",
  "css": { ... },
  "litegraph": { ... }
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | `string` | 主题唯一标识，只能包含字母、数字、下划线和连字符。 |
| `display_name` | `string` | 在主题选择窗口中显示的名称。 |
| `mode` | `"dark" \| "light"` | 主题模式，目前仅用于分类展示。 |
| `css` | `Record<string, string>` | CSS 自定义属性映射，键名为 `--xxx`，值为颜色字符串。 |
| `litegraph` | `object` | LiteGraph 画布配色对象，见下文。 |

---

## CSS 变量参考

以下是所有可用的 CSS 变量及其作用。所有值均为 CSS 合法颜色字符串（`#rrggbb`、`rgb(...)`、`rgba(...)` 等）。

### 基础背景与文字

| 变量名 | 作用范围 |
|---|---|
| `--bg` | 页面主背景色（画布外区域、对话框背景）。 |
| `--bg-deep` | 深层背景色（侧边栏、面板底层）。 |
| `--toolbar-bg` | 顶部工具栏背景。 |
| `--text` | 主文字颜色。 |
| `--text-muted` | 次要文字颜色（说明文字、标签）。 |
| `--text-dim` | 暗淡文字（占位符、禁用状态）。 |
| `--text-faint` | 微弱文字（分割线文字、极次要信息）。 |
| `--text-faint2` | 更微弱的文字/边框（通常用于分割线）。 |

### 交互与状态

| 变量名 | 作用范围 |
|---|---|
| `--accent` | 强调色（高亮边框、活跃指示器、选中标记）。 |
| `--accent-subtle` | 强调色的半透明淡底（选中行背景、微高亮）。 |
| `--border` | 通用边框颜色。 |
| `--node-hover` | 节点/列表项悬停时的背景色。 |
| `--tab-inactive` | 非活动标签页背景。 |
| `--link` | 超链接颜色。 |
| `--run-color` | 运行状态/成功指示色。 |

### 表单与按钮

| 变量名 | 作用范围 |
|---|---|
| `--input-bg` | 输入框、下拉框背景。 |
| `--btn-bg` | 普通按钮背景。 |
| `--btn-hover` | 普通按钮悬停背景。 |
| `--btn-primary` | 主按钮背景（保存、确认等）。 |
| `--btn-primary-hover` | 主按钮悬停背景。 |
| `--btn-primary-text` | 主按钮文字颜色。 |

### 面板与卡片

| 变量名 | 作用范围 |
|---|---|
| `--tool-card-bg` | 右侧工具面板卡片背景。 |
| `--tool-card-summary` | 工具卡片摘要/标题颜色。 |
| `--float-bg` | 浮动菜单、上下文菜单背景（通常带透明度）。 |
| `--toast-text` | 通知提示文字颜色。 |
| `--log-stream-bg` | 日志流面板背景。 |

### 日志徽章

日志流中的级别标签使用以下成对变量：

| 变量名 | 作用范围 |
|---|---|
| `--badge-info-bg` / `--badge-info-text` | Info 级别徽章。 |
| `--badge-warn-bg` / `--badge-warn-text` | Warn 级别徽章。 |
| `--badge-error-bg` / `--badge-error-text` | Error 级别徽章。 |
| `--badge-debug-bg` / `--badge-debug-text` | Debug 级别徽章。 |
| `--badge-trace-bg` / `--badge-trace-text` | Trace 级别徽章。 |

---

## LiteGraph 颜色参考

LiteGraph 颜色控制画布上的节点图渲染。所有值均为颜色字符串。

### 画布与网格

| 字段名 | 作用范围 |
|---|---|
| `canvasBg` | 画布背景色。 |
| `gridDotColor` | 网格点颜色。 |

### 节点外观

| 字段名 | 作用范围 |
|---|---|
| `nodeBg` | 节点主体背景。 |
| `nodeHeader` | 节点标题栏背景。 |
| `nodeTitleText` | 节点标题文字。 |
| `nodeSelectedTitle` | 选中节点标题文字。 |
| `nodeText` | 节点内容文字。 |
| `nodeBox` | 节点选中框填充色。 |
| `nodeBoxOutline` | 节点选中框描边色。 |
| `shadow` | 节点阴影（通常使用 `rgba`）。 |

### 控件与小组件

| 字段名 | 作用范围 |
|---|---|
| `widgetBg` | 节点内控件（输入框、滑块等）背景。 |
| `widgetOutline` | 控件描边。 |
| `widgetText` | 控件文字。 |
| `widgetSecondary` | 控件次要文字（单位、提示）。 |
| `widgetDisabled` | 控件禁用状态文字。 |
| `widgetButtonBg` | 节点内按钮背景。 |
| `widgetButtonText` | 节点内按钮文字。 |

### 连线与边

| 字段名 | 作用范围 |
|---|---|
| `linkColor` | 普通数据连线颜色。 |
| `eventLinkColor` | 事件触发连线颜色。 |
| `connectingLinkColor` | 正在拖拽中的连线颜色。 |
| `linkHalo` | 连线周围的光晕/描边（通常使用 `rgba`）。 |
| `linkLabelBg` | 连线标签背景（通常使用 `rgba`）。 |
| `linkLabelText` | 连线标签文字。 |

### 特殊节点

| 字段名 | 作用范围 |
|---|---|
| `boundaryNodeHeader` | 边界节点（如子图入口/出口）标题栏。 |
| `boundaryNodeBg` | 边界节点主体背景。 |

### 端口类型颜色

`linkTypeColors` 是一个子对象，定义不同类型端口的默认颜色：

| 字段名 | 对应类型 |
|---|---|
| `primitive` | 基础类型（`String`, `Int`, `Float`, `Bool` 等）。 |
| `complex` | 复杂类型（`Json`, `MessageEvent`, `OpenAIMessage`, `FunctionTools`, `LLModel` 等）。 |
| `ref` | 引用类型（以 `Ref` 结尾，如 `LoopControlRef`）。 |
| `array` | 数组类型（以 `Vec` 开头）。 |
| `any` | 任意类型（`Any` 或 `*`）。 |

---

## 完整示例

以下是一个完整的暗色主题示例，保存为 `custom_themes/example_dark.json`：

```json
{
  "name": "example_dark",
  "display_name": "示例暗色",
  "mode": "dark",
  "css": {
    "--bg": "#0d0d0d",
    "--bg-deep": "#141414",
    "--toolbar-bg": "#1a1a1a",
    "--text": "#e6e6e6",
    "--text-muted": "#a0a0a0",
    "--text-dim": "#707070",
    "--text-faint": "#505050",
    "--text-faint2": "#383838",
    "--accent": "#3b82f6",
    "--accent-subtle": "rgba(59, 130, 246, 0.12)",
    "--border": "#222222",
    "--node-hover": "#1c1c1c",
    "--tab-inactive": "#252525",
    "--link": "#60a5fa",
    "--run-color": "#22c55e",
    "--input-bg": "#0a0a0a",
    "--btn-bg": "#1e1e1e",
    "--btn-hover": "#2a2a2a",
    "--btn-primary": "#2563eb",
    "--btn-primary-hover": "#3b82f6",
    "--btn-primary-text": "#ffffff",
    "--tool-card-bg": "#0a0a0a",
    "--tool-card-summary": "#60a5fa",
    "--float-bg": "rgba(13, 13, 13, 0.95)",
    "--toast-text": "#e6e6e6",
    "--log-stream-bg": "#0a0a0a",
    "--badge-info-bg": "#0d2d1a",
    "--badge-info-text": "#4ade80",
    "--badge-warn-bg": "#3a2a00",
    "--badge-warn-text": "#facc15",
    "--badge-error-bg": "#3a0000",
    "--badge-error-text": "#f87171",
    "--badge-debug-bg": "#0a1a3a",
    "--badge-debug-text": "#60a5fa",
    "--badge-trace-bg": "#1a1a1a",
    "--badge-trace-text": "#a0a0a0"
  },
  "litegraph": {
    "canvasBg": "#0d0d0d",
    "gridDotColor": "#1a1a1a",
    "nodeBg": "#141414",
    "nodeHeader": "#1e3a5f",
    "nodeTitleText": "#e6e6e6",
    "nodeSelectedTitle": "#ffffff",
    "nodeText": "#a0a0a0",
    "nodeBox": "#2563eb",
    "nodeBoxOutline": "#3b82f6",
    "shadow": "rgba(0,0,0,0.6)",
    "widgetBg": "#0a0a0a",
    "widgetOutline": "#2a2a2a",
    "widgetText": "#e6e6e6",
    "widgetSecondary": "#707070",
    "widgetDisabled": "#404040",
    "widgetButtonBg": "#1e3a5f",
    "widgetButtonText": "#e6e6e6",
    "linkColor": "#888888",
    "eventLinkColor": "#aaaaaa",
    "connectingLinkColor": "#ffffff",
    "linkHalo": "rgba(8, 8, 14, 0.55)",
    "linkLabelBg": "rgba(6, 6, 10, 0.78)",
    "linkLabelText": "#ffffff",
    "boundaryNodeHeader": "#0d4d48",
    "boundaryNodeBg": "#051f1c",
    "linkTypeColors": {
      "primitive": "#60a5fa",
      "complex": "#fbbf24",
      "ref": "#4ade80",
      "array": "#c084fc",
      "any": "#888888"
    }
  }
}
```

保存文件后重启服务，点击编辑器左上角 **Zihuan Next** → **主题**，即可在列表中看到"示例暗色"。

---

## 调试技巧

1. **实时预览**：在主题选择窗口中，鼠标悬停在某个主题上会出现预览浮窗，显示该主题的配色效果，无需点击即可确认。
2. **错误容忍**：如果某个主题 JSON 文件格式错误，后端会跳过该文件，不会导致其他主题或服务崩溃。检查终端日志可看到加载失败的文件名。
3. **变量遗漏**：CSS 变量和 LiteGraph 字段均可省略。省略时，画布区域会使用内置默认主题的对应值；DOM 区域则依赖浏览器对未定义 CSS 变量的回退处理。
4. **最小主题**：你可以只写想覆盖的变量。例如，仅修改强调色和节点标题栏的最小主题：
   ```json
   {
     "name": "minimal_blue",
     "display_name": "极简蓝色",
     "mode": "dark",
     "css": {
       "--accent": "#3b82f6"
     },
     "litegraph": {
       "nodeHeader": "#1e3a5f"
     }
   }
   ```
