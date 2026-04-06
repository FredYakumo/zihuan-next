// Modal dialog implementations for special node editors

import { graphs } from "../api/client";
import type { NodeDefinition, NodeGraphDefinition, Port } from "../api/types";

// ─── Data types ──────────────────────────────────────────────────────────────

export interface FunctionPortDef {
  name: string;
  data_type: string;
}

export interface EmbeddedFunctionConfig {
  name: string;
  description: string;
  inputs: FunctionPortDef[];
  outputs: FunctionPortDef[];
  subgraph: NodeGraphDefinition;
}

export interface ToolParamDef {
  name: string;
  data_type: string;
  desc: string;
}

export interface BrainToolDefinition {
  id: string;
  name: string;
  description: string;
  parameters: ToolParamDef[];
  outputs: FunctionPortDef[];
  subgraph: NodeGraphDefinition;
}

// ─── Styles ──────────────────────────────────────────────────────────────────

const DIALOG_STYLES = `
  .zh-overlay {
    position: fixed; inset: 0; background: rgba(0,0,0,0.6);
    display: flex; align-items: center; justify-content: center;
    z-index: 9999; font-family: sans-serif;
  }
  .zh-dialog {
    background: #1a1a2e; border: 1px solid #2a2a4a; border-radius: 8px;
    padding: 20px; min-width: 480px; max-width: 720px; max-height: 80vh;
    overflow-y: auto; color: #e0e0e0; box-shadow: 0 8px 32px rgba(0,0,0,0.5);
  }
  .zh-dialog h3 { margin: 0 0 12px; font-size: 15px; color: #8ab4f8; }
  .zh-dialog label { display: block; font-size: 12px; color: #aaa; margin-bottom: 3px; }
  .zh-dialog input, .zh-dialog textarea, .zh-dialog select {
    width: 100%; box-sizing: border-box; padding: 6px 8px;
    background: #0f1a2e; border: 1px solid #2a2a4a; border-radius: 4px;
    color: #e0e0e0; font-size: 13px; margin-bottom: 10px;
  }
  .zh-dialog textarea { resize: vertical; min-height: 80px; font-family: monospace; }
  .zh-dialog .zh-row { display: flex; gap: 8px; align-items: center; margin-bottom: 8px; }
  .zh-dialog .zh-row input, .zh-dialog .zh-row select { margin-bottom: 0; }
  .zh-dialog .zh-buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
  .zh-dialog button {
    padding: 6px 16px; border-radius: 4px; border: 1px solid #2a2a4a;
    background: #1e3a5f; color: #e0e0e0; cursor: pointer; font-size: 13px;
  }
  .zh-dialog button:hover { background: #1a3a6e; }
  .zh-dialog button.primary { background: #0a5a9e; border-color: #1a7abf; }
  .zh-dialog button.primary:hover { background: #0a6abf; }
  .zh-dialog button.danger { color: #e94560; border-color: #e94560; background: transparent; }
  .zh-dialog button.danger:hover { background: rgba(233,69,96,0.1); }
  .zh-dialog .zh-section-label {
    font-size: 12px; font-weight: bold; color: #8ab4f8;
    margin: 12px 0 6px; padding-bottom: 4px; border-bottom: 1px solid #2a2a4a;
  }
  .zh-dialog .zh-port-row { display: flex; gap: 6px; align-items: center; margin-bottom: 6px; }
  .zh-dialog .zh-port-row input { flex: 1; margin-bottom: 0; }
  .zh-dialog .zh-port-row select { flex: 1; margin-bottom: 0; }
  .zh-dialog .zh-tool-card {
    border: 1px solid #2a2a4a; border-radius: 6px; padding: 10px; margin-bottom: 10px; background: #0d1020;
  }
  .zh-dialog .zh-tool-card summary {
    cursor: pointer; font-size: 13px; font-weight: bold; color: #cdf; list-style: none;
  }
  .zh-dialog .zh-hint { font-size: 11px; color: #888; margin-bottom: 10px; }
`;

function ensureDialogStyles(): void {
  if (document.getElementById("zh-dialog-styles")) return;
  const style = document.createElement("style");
  style.id = "zh-dialog-styles";
  style.textContent = DIALOG_STYLES;
  document.head.appendChild(style);
}

const DATA_TYPES = [
  "String", "Integer", "Float", "Boolean", "Json", "Any",
  "MessageEvent", "OpenAIMessage", "QQMessage", "FunctionTools",
  "BotAdapterRef", "RedisRef", "MySqlRef", "TavilyRef",
  "SessionStateRef", "OpenAIMessageSessionCacheRef", "Password", "LLModel",
  "LoopControlRef", "Binary",
];

function dataTypeSelect(value = "String", id?: string): HTMLSelectElement {
  const sel = document.createElement("select");
  if (id) sel.id = id;
  for (const dt of DATA_TYPES) {
    const opt = document.createElement("option");
    opt.value = dt;
    opt.textContent = dt;
    if (dt === value) opt.selected = true;
    sel.appendChild(opt);
  }
  return sel;
}

function openOverlay(): { overlay: HTMLDivElement; dialog: HTMLDivElement; close: () => void } {
  ensureDialogStyles();
  const overlay = document.createElement("div");
  overlay.className = "zh-overlay";
  const dialog = document.createElement("div");
  dialog.className = "zh-dialog";
  overlay.appendChild(dialog);
  document.body.appendChild(overlay);
  const close = () => overlay.remove();
  return { overlay, dialog, close };
}

// ─── Port list editor ─────────────────────────────────────────────────────────

function buildPortListEditor(
  container: HTMLElement,
  ports: FunctionPortDef[],
  showDesc?: boolean
): () => FunctionPortDef[] {
  const items: Array<{ nameEl: HTMLInputElement; typeEl: HTMLSelectElement; descEl?: HTMLInputElement }> = [];

  const addRow = (port?: FunctionPortDef) => {
    const row = document.createElement("div");
    row.className = "zh-port-row";

    const nameEl = document.createElement("input");
    nameEl.type = "text";
    nameEl.placeholder = "port_name";
    nameEl.value = port?.name ?? "";

    const typeEl = dataTypeSelect(port?.data_type ?? "String");

    const removeBtn = document.createElement("button");
    removeBtn.textContent = "✕";
    removeBtn.className = "danger";
    removeBtn.style.padding = "4px 8px";
    removeBtn.addEventListener("click", () => {
      const i = items.findIndex(it => it.nameEl === nameEl);
      if (i >= 0) items.splice(i, 1);
      row.remove();
    });

    row.appendChild(nameEl);
    row.appendChild(typeEl);
    row.appendChild(removeBtn);
    container.insertBefore(row, addBtn);

    items.push({ nameEl, typeEl });
  };

  const addBtn = document.createElement("button");
  addBtn.textContent = "+ 添加端口";
  addBtn.style.marginBottom = "8px";
  addBtn.addEventListener("click", () => addRow());
  container.appendChild(addBtn);

  for (const p of ports) addRow(p);

  return () => items
    .map(it => ({ name: it.nameEl.value.trim(), data_type: it.typeEl.value }))
    .filter(p => p.name);
}

// ─── Format String Editor ────────────────────────────────────────────────────

export function openFormatStringEditor(
  nodeDef: NodeDefinition,
  sessionId: string,
  onSaved: () => void
): void {
  const { dialog, close } = openOverlay();

  const currentTemplate = (nodeDef.inline_values?.["template"] as string) ?? "";
  dialog.style.minWidth = "640px";

  dialog.innerHTML = `
    <h3>编辑格式化文本</h3>
    <div class="zh-hint">使用 <code>\${变量名}</code> 引用输入变量，保存后自动更新输入端口</div>
    <div style="display:flex;gap:12px;min-height:220px;">
      <div style="width:150px;flex-shrink:0;">
        <div class="zh-section-label">变量列表</div>
        <div id="zh-var-list" style="display:flex;flex-direction:column;gap:4px;padding-top:4px;"></div>
      </div>
      <div style="flex:1;position:relative;">
        <textarea id="zh-template-input" style="width:100%;min-height:200px;box-sizing:border-box;margin:0;"></textarea>
        <div id="zh-autocomplete" style="
          position:absolute;display:none;z-index:10001;
          background:#1a1a2e;border:1px solid #2a2a4a;border-radius:4px;
          min-width:160px;box-shadow:0 4px 16px rgba(0,0,0,0.6);overflow:hidden;
        "></div>
      </div>
    </div>
  `;

  const ta = dialog.querySelector<HTMLTextAreaElement>("#zh-template-input")!;
  const varList = dialog.querySelector<HTMLDivElement>("#zh-var-list")!;
  const autocomplete = dialog.querySelector<HTMLDivElement>("#zh-autocomplete")!;
  ta.value = currentTemplate;

  let currentVars: string[] = extractTemplateVars(currentTemplate);

  // Refresh clickable variable pills in left panel
  function refreshVarList(vars: string[]): void {
    varList.innerHTML = "";
    for (const v of vars) {
      const btn = document.createElement("button");
      btn.textContent = `\${${v}}`;
      btn.title = `点击在光标处插入 \${${v}}`;
      btn.style.cssText =
        "width:100%;text-align:left;padding:4px 8px;font-family:monospace;font-size:12px;background:#0a5a9e;border-color:#1a7abf;";
      btn.addEventListener("click", () => insertAtCursor(`\${${v}}`));
      varList.appendChild(btn);
    }
  }

  function insertAtCursor(text: string): void {
    const start = ta.selectionStart;
    const end = ta.selectionEnd;
    ta.value = ta.value.substring(0, start) + text + ta.value.substring(end);
    ta.selectionStart = ta.selectionEnd = start + text.length;
    ta.dispatchEvent(new Event("input"));
    ta.focus();
  }

  function hideAutoComplete(): void {
    autocomplete.style.display = "none";
    autocomplete.innerHTML = "";
  }

  function updateAutoComplete(): void {
    const cursor = ta.selectionStart;
    const before = ta.value.substring(0, cursor);
    const openIdx = before.lastIndexOf("${");
    if (openIdx < 0) { hideAutoComplete(); return; }

    const afterOpen = before.substring(openIdx + 2);
    if (afterOpen.includes("}")) { hideAutoComplete(); return; }

    const prefix = afterOpen.toLowerCase();
    const matches = currentVars.filter(v => v.toLowerCase().startsWith(prefix));
    if (matches.length === 0) { hideAutoComplete(); return; }

    autocomplete.innerHTML = "";
    // Position dropdown just below the textarea
    autocomplete.style.top = ta.offsetTop + ta.offsetHeight + "px";
    autocomplete.style.left = ta.offsetLeft + "px";
    autocomplete.style.maxWidth = ta.offsetWidth + "px";
    autocomplete.style.display = "block";

    for (const v of matches) {
      const item = document.createElement("div");
      item.textContent = v;
      item.style.cssText =
        "padding:6px 12px;cursor:pointer;font-family:monospace;font-size:13px;color:#8ab4f8;border-bottom:1px solid #2a2a4a;";
      item.addEventListener("mousedown", (e) => {
        e.preventDefault(); // keep textarea focus
        const full = ta.value;
        const newText =
          full.substring(0, openIdx) + `\${${v}}` + full.substring(cursor);
        ta.value = newText;
        ta.selectionStart = ta.selectionEnd = openIdx + v.length + 3;
        hideAutoComplete();
        ta.dispatchEvent(new Event("input"));
        ta.focus();
      });
      item.addEventListener("mouseenter", () => { (item.style.background = "#1a3a6e"); });
      item.addEventListener("mouseleave", () => { (item.style.background = ""); });
      autocomplete.appendChild(item);
    }
  }

  ta.addEventListener("input", () => {
    currentVars = extractTemplateVars(ta.value);
    refreshVarList(currentVars);
    updateAutoComplete();
  });
  ta.addEventListener("keyup", updateAutoComplete);
  ta.addEventListener("mouseup", updateAutoComplete);
  ta.addEventListener("blur", () => setTimeout(hideAutoComplete, 150));

  refreshVarList(currentVars);

  // --- Buttons ---
  const btns = document.createElement("div");
  btns.className = "zh-buttons";

  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  cancelBtn.addEventListener("click", close);

  const saveBtn = document.createElement("button");
  saveBtn.textContent = "保存";
  saveBtn.className = "primary";
  saveBtn.addEventListener("click", async () => {
    const template = ta.value;
    const varNames = extractTemplateVars(template);

    try {
      const graphDef = await graphs.get(sessionId);
      const nodeIdx = graphDef.nodes.findIndex(n => n.id === nodeDef.id);
      if (nodeIdx < 0) { alert("Node not found"); close(); return; }

      const currentNode = graphDef.nodes[nodeIdx];

      // "template" is the one static input port — keep it, replace everything else
      const staticPorts = currentNode.input_ports.filter(p => p.name === "template");
      const newDynamicPorts: Port[] = varNames.map(name => ({
        name,
        data_type: "Any",               // matches Rust: DataType::Any
        description: `变量 ${name}`,
        required: false,
      }));

      const updatedNode = {
        ...currentNode,
        input_ports: [...staticPorts, ...newDynamicPorts],
        inline_values: { ...currentNode.inline_values, template },
      };
      const updatedGraph = {
        ...graphDef,
        nodes: graphDef.nodes.map((n, i) => (i === nodeIdx ? updatedNode : n)),
      };

      await graphs.put(sessionId, updatedGraph);
      close();
      onSaved();
    } catch (e) {
      alert("保存失败: " + (e as Error).message);
    }
  });

  btns.appendChild(cancelBtn);
  btns.appendChild(saveBtn);
  dialog.appendChild(btns);
}

// ─── JSON Extract Editor ─────────────────────────────────────────────────────

interface JsonFieldDef {
  name: string;
  data_type: string;
}

export function openJsonExtractEditor(
  nodeDef: NodeDefinition,
  sessionId: string,
  onSaved: () => void
): void {
  const { dialog, close } = openOverlay();

  const currentFields: JsonFieldDef[] =
    (nodeDef.inline_values?.["fields_config"] as JsonFieldDef[] | undefined) ?? [];

  dialog.innerHTML = `<h3>配置 JSON 提取字段</h3>
    <div class="zh-hint">每个字段对应一个输出端口，保存后将自动更新输出端口。</div>
    <div class="zh-section-label">输出字段</div>
  `;

  const listContainer = document.createElement("div");
  const items: Array<{ nameEl: HTMLInputElement; typeEl: HTMLSelectElement }> = [];

  const addRow = (field?: JsonFieldDef) => {
    const row = document.createElement("div");
    row.className = "zh-port-row";

    const nameEl = document.createElement("input");
    nameEl.type = "text";
    nameEl.placeholder = "字段名 (如 result)";
    nameEl.value = field?.name ?? "";

    const typeEl = dataTypeSelect(field?.data_type ?? "String");

    const removeBtn = document.createElement("button");
    removeBtn.textContent = "✕";
    removeBtn.className = "danger";
    removeBtn.style.padding = "4px 8px";
    removeBtn.addEventListener("click", () => {
      const i = items.findIndex(it => it.nameEl === nameEl);
      if (i >= 0) items.splice(i, 1);
      row.remove();
    });

    row.appendChild(nameEl);
    row.appendChild(typeEl);
    row.appendChild(removeBtn);
    listContainer.insertBefore(row, addRowBtn);

    items.push({ nameEl, typeEl });
  };

  const addRowBtn = document.createElement("button");
  addRowBtn.textContent = "+ 添加字段";
  addRowBtn.style.marginBottom = "8px";
  addRowBtn.addEventListener("click", () => addRow());
  listContainer.appendChild(addRowBtn);

  for (const f of currentFields) addRow(f);
  dialog.appendChild(listContainer);

  const btns = document.createElement("div");
  btns.className = "zh-buttons";

  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  cancelBtn.addEventListener("click", close);

  const saveBtn = document.createElement("button");
  saveBtn.textContent = "保存";
  saveBtn.className = "primary";
  saveBtn.addEventListener("click", async () => {
    const fields = items
      .map(it => ({ name: it.nameEl.value.trim(), data_type: it.typeEl.value }))
      .filter(f => f.name);

    try {
      const graphDef = await graphs.get(sessionId);
      const nodeIdx = graphDef.nodes.findIndex(n => n.id === nodeDef.id);
      if (nodeIdx < 0) { alert("Node not found"); close(); return; }

      const newOutputPorts: Port[] = fields.map(f => ({
        name: f.name,
        data_type: f.data_type,
        description: null,
        required: false,
      }));

      const updatedNode = {
        ...graphDef.nodes[nodeIdx],
        output_ports: newOutputPorts,
        inline_values: {
          ...graphDef.nodes[nodeIdx].inline_values,
          fields_config: fields,
        },
      };
      const updatedGraph = {
        ...graphDef,
        nodes: graphDef.nodes.map((n, i) => i === nodeIdx ? updatedNode : n),
      };

      await graphs.put(sessionId, updatedGraph);
      close();
      onSaved();
    } catch (e) {
      alert("保存失败: " + (e as Error).message);
    }
  });

  btns.appendChild(cancelBtn);
  btns.appendChild(saveBtn);
  dialog.appendChild(btns);
}

// ─── Function Signature Editor ───────────────────────────────────────────────

export function openFunctionSignatureEditor(
  nodeDef: NodeDefinition,
  sessionId: string,
  onSaved: () => void,
  onEditSubgraph: (config: EmbeddedFunctionConfig) => void
): void {
  const { dialog, close } = openOverlay();

  const cfg: EmbeddedFunctionConfig = {
    name: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.name) ?? nodeDef.name,
    description: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.description) ?? "",
    inputs: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.inputs) ?? [],
    outputs: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.outputs) ?? [],
    subgraph: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.subgraph) ?? { nodes: [], edges: [], hyperparameter_groups: [], hyperparameters: [], variables: [] } as any,
  };

  dialog.innerHTML = `
    <h3>编辑函数签名</h3>
    <label>函数名</label>
    <input type="text" id="zh-fn-name" value="${escapeHtml(cfg.name)}">
    <label>描述</label>
    <input type="text" id="zh-fn-desc" value="${escapeHtml(cfg.description)}">
    <div class="zh-section-label">输入端口</div>
  `;

  const inputsContainer = document.createElement("div");
  const getInputs = buildPortListEditor(inputsContainer, cfg.inputs);
  dialog.appendChild(inputsContainer);

  const outLabel = document.createElement("div");
  outLabel.className = "zh-section-label";
  outLabel.textContent = "输出端口";
  dialog.appendChild(outLabel);

  const outputsContainer = document.createElement("div");
  const getOutputs = buildPortListEditor(outputsContainer, cfg.outputs);
  dialog.appendChild(outputsContainer);

  const btns = document.createElement("div");
  btns.className = "zh-buttons";

  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  cancelBtn.addEventListener("click", close);

  const editSubgraphBtn = document.createElement("button");
  editSubgraphBtn.textContent = "↳ 编辑子图";
  editSubgraphBtn.addEventListener("click", async () => {
    const nameEl = dialog.querySelector<HTMLInputElement>("#zh-fn-name")!;
    const descEl = dialog.querySelector<HTMLInputElement>("#zh-fn-desc")!;
    const currentCfg: EmbeddedFunctionConfig = {
      ...cfg,
      name: nameEl.value.trim(),
      description: descEl.value.trim(),
      inputs: getInputs(),
      outputs: getOutputs(),
    };
    close();
    onEditSubgraph(currentCfg);
  });

  const saveBtn = document.createElement("button");
  saveBtn.textContent = "保存签名";
  saveBtn.className = "primary";
  saveBtn.addEventListener("click", async () => {
    const nameEl = dialog.querySelector<HTMLInputElement>("#zh-fn-name")!;
    const descEl = dialog.querySelector<HTMLInputElement>("#zh-fn-desc")!;

    const updatedConfig: EmbeddedFunctionConfig = {
      ...cfg,
      name: nameEl.value.trim(),
      description: descEl.value.trim(),
      inputs: getInputs(),
      outputs: getOutputs(),
    };

    try {
      await graphs.updateNode(sessionId, nodeDef.id, {
        inline_values: { function_config: updatedConfig as unknown as Record<string, unknown> },
      });
      close();
      onSaved();
    } catch (e) {
      alert("保存失败: " + (e as Error).message);
    }
  });

  btns.appendChild(cancelBtn);
  btns.appendChild(editSubgraphBtn);
  btns.appendChild(saveBtn);
  dialog.appendChild(btns);
}

// ─── Brain Tools Manager ─────────────────────────────────────────────────────

export function openBrainToolsEditor(
  nodeDef: NodeDefinition,
  sessionId: string,
  onSaved: () => void,
  onEditToolSubgraph: (toolIndex: number, tool: BrainToolDefinition) => void
): void {
  const { dialog, close } = openOverlay();

  const rawTools = (nodeDef.inline_values?.["tools_config"] as BrainToolDefinition[] | undefined) ?? [];
  const rawSharedInputs = (nodeDef.inline_values?.["shared_inputs"] as FunctionPortDef[] | undefined) ?? [];

  // Deep copy to avoid mutating original
  const tools: BrainToolDefinition[] = JSON.parse(JSON.stringify(rawTools));
  const sharedInputs: FunctionPortDef[] = JSON.parse(JSON.stringify(rawSharedInputs));

  const render = () => {
    dialog.innerHTML = `<h3>管理 Brain 工具</h3>
      <div class="zh-section-label">共享输入端口 (shared_inputs)</div>
    `;

    const sharedContainer = document.createElement("div");
    const getSharedInputs = buildPortListEditor(sharedContainer, sharedInputs);
    dialog.appendChild(sharedContainer);

    const toolsLabel = document.createElement("div");
    toolsLabel.className = "zh-section-label";
    toolsLabel.textContent = `工具列表 (${tools.length})`;
    dialog.appendChild(toolsLabel);

    const toolsContainer = document.createElement("div");
    tools.forEach((tool, idx) => {
      toolsContainer.appendChild(buildToolCard(tool, idx));
    });
    dialog.appendChild(toolsContainer);

    const addToolBtn = document.createElement("button");
    addToolBtn.textContent = "+ 添加工具";
    addToolBtn.style.marginBottom = "12px";
    addToolBtn.addEventListener("click", () => {
      tools.push({
        id: `tool_${Date.now()}`,
        name: `tool_${tools.length + 1}`,
        description: "",
        parameters: [],
        outputs: [],
        subgraph: { nodes: [], edges: [], hyperparameter_groups: [], hyperparameters: [], variables: [] } as any,
      });
      close();
      openBrainToolsEditor(
        { ...nodeDef, inline_values: { ...nodeDef.inline_values, tools_config: tools, shared_inputs: getSharedInputs() } },
        sessionId, onSaved, onEditToolSubgraph
      );
    });
    dialog.appendChild(addToolBtn);

    const btns = document.createElement("div");
    btns.className = "zh-buttons";

    const cancelBtn = document.createElement("button");
    cancelBtn.textContent = "取消";
    cancelBtn.addEventListener("click", close);

    const saveBtn = document.createElement("button");
    saveBtn.textContent = "保存";
    saveBtn.className = "primary";
    saveBtn.addEventListener("click", async () => {
      try {
        await graphs.updateNode(sessionId, nodeDef.id, {
          inline_values: {
            tools_config: tools as unknown as unknown[],
            shared_inputs: getSharedInputs() as unknown as unknown[],
          } as Record<string, unknown>,
        });
        close();
        onSaved();
      } catch (e) {
        alert("保存失败: " + (e as Error).message);
      }
    });

    btns.appendChild(cancelBtn);
    btns.appendChild(saveBtn);
    dialog.appendChild(btns);
  };

  const buildToolCard = (tool: BrainToolDefinition, idx: number): HTMLElement => {
    const card = document.createElement("div");
    card.className = "zh-tool-card";

    // Header row
    const headerRow = document.createElement("div");
    headerRow.style.cssText = "display:flex;align-items:center;gap:8px;margin-bottom:8px;";

    const nameInput = document.createElement("input");
    nameInput.type = "text";
    nameInput.value = tool.name;
    nameInput.placeholder = "工具名";
    nameInput.style.cssText = "flex:1;margin-bottom:0;";
    nameInput.addEventListener("change", () => { tools[idx].name = nameInput.value.trim(); });

    const deleteBtn = document.createElement("button");
    deleteBtn.textContent = "✕ 删除";
    deleteBtn.className = "danger";
    deleteBtn.style.padding = "4px 8px";
    deleteBtn.addEventListener("click", () => {
      tools.splice(idx, 1);
      card.remove();
    });

    headerRow.appendChild(nameInput);
    headerRow.appendChild(deleteBtn);
    card.appendChild(headerRow);

    // Description
    const descLabel = document.createElement("label");
    descLabel.textContent = "描述";
    const descInput = document.createElement("input");
    descInput.type = "text";
    descInput.value = tool.description;
    descInput.placeholder = "工具描述";
    descInput.addEventListener("change", () => { tools[idx].description = descInput.value.trim(); });
    card.appendChild(descLabel);
    card.appendChild(descInput);

    // Parameters
    const paramLabel = document.createElement("div");
    paramLabel.className = "zh-section-label";
    paramLabel.textContent = "参数 (parameters)";
    card.appendChild(paramLabel);

    const paramContainer = document.createElement("div");
    const paramItems: Array<{ nameEl: HTMLInputElement; typeEl: HTMLSelectElement; descEl: HTMLInputElement }> = [];

    const addParamRow = (param?: ToolParamDef) => {
      const row = document.createElement("div");
      row.className = "zh-port-row";

      const nameEl = document.createElement("input");
      nameEl.type = "text";
      nameEl.placeholder = "参数名";
      nameEl.value = param?.name ?? "";

      const typeEl = dataTypeSelect(param?.data_type ?? "String");

      const descEl = document.createElement("input");
      descEl.type = "text";
      descEl.placeholder = "参数说明";
      descEl.value = param?.desc ?? "";
      descEl.style.flex = "2";

      const removeBtn = document.createElement("button");
      removeBtn.textContent = "✕";
      removeBtn.className = "danger";
      removeBtn.style.padding = "4px 8px";
      removeBtn.addEventListener("click", () => {
        const i = paramItems.findIndex(it => it.nameEl === nameEl);
        if (i >= 0) paramItems.splice(i, 1);
        row.remove();
        syncParams();
      });

      row.appendChild(nameEl);
      row.appendChild(typeEl);
      row.appendChild(descEl);
      row.appendChild(removeBtn);
      paramContainer.insertBefore(row, addParamBtn);

      paramItems.push({ nameEl, typeEl, descEl });
      [nameEl, typeEl, descEl].forEach(el => el.addEventListener("change", syncParams));
    };

    const syncParams = () => {
      tools[idx].parameters = paramItems
        .map(it => ({ name: it.nameEl.value.trim(), data_type: it.typeEl.value, desc: it.descEl.value.trim() }))
        .filter(p => p.name);
    };

    const addParamBtn = document.createElement("button");
    addParamBtn.textContent = "+ 添加参数";
    addParamBtn.style.marginBottom = "8px";
    addParamBtn.addEventListener("click", () => addParamRow());
    paramContainer.appendChild(addParamBtn);

    for (const p of tool.parameters) addParamRow(p);
    card.appendChild(paramContainer);

    // Outputs
    const outLabel = document.createElement("div");
    outLabel.className = "zh-section-label";
    outLabel.textContent = "输出 (outputs)";
    card.appendChild(outLabel);

    const outContainer = document.createElement("div");
    const getOutputs = buildPortListEditor(outContainer, tool.outputs);
    outContainer.addEventListener("input", () => { tools[idx].outputs = getOutputs(); });
    card.appendChild(outContainer);

    // Edit subgraph button
    const editSubBtn = document.createElement("button");
    editSubBtn.textContent = "↳ 编辑工具子图";
    editSubBtn.style.marginTop = "6px";
    editSubBtn.addEventListener("click", () => {
      // Sync current state before entering subgraph
      tools[idx].name = nameInput.value.trim();
      tools[idx].description = descInput.value.trim();
      syncParams();
      tools[idx].outputs = getOutputs();
      close();
      onEditToolSubgraph(idx, tools[idx]);
    });
    card.appendChild(editSubBtn);

    return card;
  };

  render();
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/** Extract ${varname} variable names from a format string template */
export function extractTemplateVars(template: string): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  const re = /\$\{([^}]+)\}/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(template)) !== null) {
    const name = m[1].trim();
    if (name && !seen.has(name)) {
      seen.add(name);
      result.push(name);
    }
  }
  return result;
}
