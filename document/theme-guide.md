# Theme Authoring Guide

> 🌐 English | [简体中文](theme-guide.zh-CN.md)

This guide explains how to write custom themes for Zihuan Next.

---

## Table of Contents

- [Theme System Overview](#theme-system-overview)
- [Theme File Location](#theme-file-location)
- [Theme File Format](#theme-file-format)
- [CSS Variables Reference](#css-variables-reference)
- [LiteGraph Colors Reference](#litegraph-colors-reference)
- [Complete Example](#complete-example)
- [Debugging Tips](#debugging-tips)

---

## Theme System Overview

Zihuan Next supports customizing the editor appearance via JSON files. A theme consists of two parts:

1. **CSS Variables**: Control DOM UI colors (toolbar, dialogs, node panels, etc.).
2. **LiteGraph Colors**: Control canvas rendering (node cards, links, grid, etc.).

The system includes two built-in default themes (`default_dark` and `default_light`) that work out of the box. You can extend more color schemes by writing theme files.

---

## Theme File Location

Create a `custom_themes/` folder in the working directory of the executable, and place your theme JSON files inside:

```
zihuan_next/
├── custom_themes/
│   ├── my_theme.json
│   └── another_theme.json
├── config.yaml
└── ...
```

After starting the service, the frontend will automatically read all theme files from this directory via `/api/themes`, and they will appear in the **Theme** selection window in the editor menu.

---

## Theme File Format

Each theme is a single JSON file with the following top-level structure:

```json
{
  "name": "my_dark",
  "display_name": "My Dark Theme",
  "mode": "dark",
  "css": { ... },
  "litegraph": { ... }
}
```

| Field | Type | Description |
|---|---|---|
| `name` | `string` | Unique theme identifier. Use only letters, digits, underscores, and hyphens. |
| `display_name` | `string` | Name shown in the theme selection window. |
| `mode` | `"dark" \| "light"` | Theme mode, currently used only for categorization. |
| `css` | `Record<string, string>` | CSS custom property map. Keys are `--xxx`, values are color strings. |
| `litegraph` | `object` | LiteGraph canvas color configuration. See below. |

---

## CSS Variables Reference

The following CSS variables are available. All values must be valid CSS color strings (`#rrggbb`, `rgb(...)`, `rgba(...)`, etc.).

### Base Backgrounds and Text

| Variable | Scope |
|---|---|
| `--bg` | Main page background (areas outside canvas, dialog background). |
| `--bg-deep` | Deep background (sidebar, panel base layer). |
| `--toolbar-bg` | Top toolbar background. |
| `--text` | Primary text color. |
| `--text-muted` | Secondary text color (descriptions, labels). |
| `--text-dim` | Dim text (placeholders, disabled state). |
| `--text-faint` | Faint text (divider labels, very minor info). |
| `--text-faint2` | Even fainter text / borders (often used for dividers). |

### Interaction and State

| Variable | Scope |
|---|---|
| `--accent` | Accent color (highlight borders, active indicators, checkmarks). |
| `--accent-subtle` | Semi-transparent accent tint (selected row background, subtle highlight). |
| `--border` | Generic border color. |
| `--node-hover` | Background color when hovering over nodes / list items. |
| `--tab-inactive` | Inactive tab background. |
| `--link` | Hyperlink color. |
| `--run-color` | Running / success indicator color. |

### Forms and Buttons

| Variable | Scope |
|---|---|
| `--input-bg` | Input box, dropdown background. |
| `--btn-bg` | Normal button background. |
| `--btn-hover` | Normal button hover background. |
| `--btn-primary` | Primary button background (save, confirm, etc.). |
| `--btn-primary-hover` | Primary button hover background. |
| `--btn-primary-text` | Primary button text color. |

### Panels and Cards

| Variable | Scope |
|---|---|
| `--tool-card-bg` | Right-side tool panel card background. |
| `--tool-card-summary` | Tool card summary / title color. |
| `--float-bg` | Floating menu, context menu background (usually with transparency). |
| `--toast-text` | Notification toast text color. |
| `--log-stream-bg` | Log stream panel background. |

### Log Badges

Log level labels in the stream use the following paired variables:

| Variable | Scope |
|---|---|
| `--badge-info-bg` / `--badge-info-text` | Info level badge. |
| `--badge-warn-bg` / `--badge-warn-text` | Warn level badge. |
| `--badge-error-bg` / `--badge-error-text` | Error level badge. |
| `--badge-debug-bg` / `--badge-debug-text` | Debug level badge. |
| `--badge-trace-bg` / `--badge-trace-text` | Trace level badge. |

---

## LiteGraph Colors Reference

LiteGraph colors control the node graph rendering on the canvas. All values are color strings.

### Canvas and Grid

| Field | Scope |
|---|---|
| `canvasBg` | Canvas background color. |
| `gridDotColor` | Grid dot color. |

### Node Appearance

| Field | Scope |
|---|---|
| `nodeBg` | Node body background. |
| `nodeHeader` | Node header bar background. |
| `nodeTitleText` | Node title text color. |
| `nodeSelectedTitle` | Selected node title text color. |
| `nodeText` | Node content text color. |
| `nodeBox` | Node selection box fill color. |
| `nodeBoxOutline` | Node selection box outline color. |
| `shadow` | Node shadow (usually `rgba`). |

### Widgets

| Field | Scope |
|---|---|
| `widgetBg` | Widget background inside nodes (inputs, sliders, etc.). |
| `widgetOutline` | Widget outline. |
| `widgetText` | Widget text color. |
| `widgetSecondary` | Widget secondary text (units, hints). |
| `widgetDisabled` | Widget disabled state text. |
| `widgetButtonBg` | Widget button background. |
| `widgetButtonText` | Widget button text color. |

### Links and Edges

| Field | Scope |
|---|---|
| `linkColor` | Normal data link color. |
| `eventLinkColor` | Event trigger link color. |
| `connectingLinkColor` | Link color while dragging a new connection. |
| `linkHalo` | Glow / outline around links (usually `rgba`). |
| `linkLabelBg` | Link label background (usually `rgba`). |
| `linkLabelText` | Link label text color. |

### Special Nodes

| Field | Scope |
|---|---|
| `boundaryNodeHeader` | Boundary node (e.g., subgraph input/output) header bar. |
| `boundaryNodeBg` | Boundary node body background. |

### Port Type Colors

`linkTypeColors` is a nested object defining default colors for different port types:

| Field | Corresponding Type |
|---|---|
| `primitive` | Primitive types (`String`, `Int`, `Float`, `Bool`, etc.). |
| `complex` | Complex types (`Json`, `MessageEvent`, `OpenAIMessage`, `FunctionTools`, `LLModel`, etc.). |
| `ref` | Reference types (types ending with `Ref`, such as `LoopControlRef`). |
| `array` | Array types (types starting with `Vec`). |
| `any` | Any type (`Any` or `*`). |

---

## Complete Example

Here is a complete dark theme example. Save it as `custom_themes/example_dark.json`:

```json
{
  "name": "example_dark",
  "display_name": "Example Dark",
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

After saving the file and restarting the service, click the top-left **Zihuan Next** → **Theme** in the editor, and you will see "Example Dark" in the list.

---

## Debugging Tips

1. **Live Preview**: In the theme selection window, hovering over a theme shows a preview tooltip with its color scheme. You can confirm the look without clicking.
2. **Error Tolerance**: If a theme JSON file is malformed, the backend will skip it without crashing the service or affecting other themes. Check the terminal logs for the failed filename.
3. **Missing Variables**: Both CSS variables and LiteGraph fields are optional. If omitted, the canvas area falls back to the corresponding value from the built-in default theme; the DOM area relies on the browser's fallback for undefined CSS variables.
4. **Minimal Theme**: You only need to write the variables you want to override. For example, a minimal theme that only changes the accent color and node header:
   ```json
   {
     "name": "minimal_blue",
     "display_name": "Minimal Blue",
     "mode": "dark",
     "css": {
       "--accent": "#3b82f6"
     },
     "litegraph": {
       "nodeHeader": "#1e3a5f"
     }
   }
   ```
