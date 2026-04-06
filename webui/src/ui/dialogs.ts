// Modal dialog implementations for special node editors

import { graphs } from "../api/client";
import type { NodeDefinition, NodeGraphDefinition, Port, HyperParameter, GraphVariable, NodeTypeInfo } from "../api/types";

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

// ─── Add Node dialog ──────────────────────────────────────────────────────────

/**
 * Show the "选择节点类型" picker dialog.
 * Resolves with the chosen `type_id` or `null` if cancelled.
 */
export function showAddNodeDialog(nodeTypes: NodeTypeInfo[]): Promise<string | null> {
  ensureDialogStyles();

  // Extra styles scoped to the add-node dialog
  const extraStyleId = "zh-add-node-styles";
  if (!document.getElementById(extraStyleId)) {
    const s = document.createElement("style");
    s.id = extraStyleId;
    s.textContent = `
      .zh-an-dialog { min-width: 540px; max-width: 760px; max-height: 82vh; display: flex; flex-direction: column; }
      .zh-an-search {
        width: 100%; box-sizing: border-box; padding: 7px 10px;
        background: #0f1a2e; border: 1px solid #3a5a8a; border-radius: 4px;
        color: #e0e0e0; font-size: 13px; margin-bottom: 10px; outline: none;
      }
      .zh-an-search:focus { border-color: #5a9af8; }
      .zh-an-tabs { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 10px; }
      .zh-an-tab {
        padding: 3px 12px; border-radius: 4px; border: 1px solid #2a2a4a;
        background: #1e3a5f; color: #aaa; cursor: pointer; font-size: 12px;
        transition: background 0.1s, color 0.1s;
      }
      .zh-an-tab:hover { background: #1a3a6e; color: #e0e0e0; }
      .zh-an-tab.active { background: #2e6abf; border-color: #3a8af8; color: #fff; }
      .zh-an-list {
        flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 4px;
        min-height: 200px; max-height: 52vh;
      }
      .zh-an-item {
        display: flex; flex-direction: column; gap: 2px;
        padding: 8px 12px; border: 1px solid #2a2a4a; border-radius: 5px;
        cursor: pointer; background: #0d1020; transition: background 0.1s;
      }
      .zh-an-item:hover { background: #1a3a6e; border-color: #3a6abf; }
      .zh-an-item-top { display: flex; align-items: center; gap: 8px; }
      .zh-an-name { font-size: 13px; font-weight: bold; color: #e0e0e0; flex: 1; }
      .zh-an-badge {
        font-size: 10px; padding: 1px 7px; border-radius: 10px;
        background: #1e3a5f; border: 1px solid #3a5a8a; color: #8ab4f8;
        white-space: nowrap;
      }
      .zh-an-desc { font-size: 11px; color: #888; line-height: 1.4; }
      .zh-an-empty { padding: 20px; text-align: center; color: #666; font-size: 13px; }
    `;
    document.head.appendChild(s);
  }

  const visibleTypes = nodeTypes.filter((n) => n.category !== "内部");
  const cats = ["全部", ...Array.from(new Set(visibleTypes.map((n) => n.category)))];

  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "zh-overlay";

    const dialog = document.createElement("div");
    dialog.className = "zh-dialog zh-an-dialog";

    const titleEl = document.createElement("h3");
    titleEl.textContent = "选择节点类型";
    dialog.appendChild(titleEl);

    const searchInput = document.createElement("input");
    searchInput.type = "text";
    searchInput.className = "zh-an-search";
    searchInput.placeholder = "输入名称、类型、分类或描述…";
    dialog.appendChild(searchInput);

    const tabsRow = document.createElement("div");
    tabsRow.className = "zh-an-tabs";
    dialog.appendChild(tabsRow);

    const listEl = document.createElement("div");
    listEl.className = "zh-an-list";
    dialog.appendChild(listEl);

    const footer = document.createElement("div");
    footer.className = "zh-buttons";
    const cancelBtn = document.createElement("button");
    cancelBtn.textContent = "取消";
    cancelBtn.addEventListener("click", () => { document.body.removeChild(overlay); resolve(null); });
    footer.appendChild(cancelBtn);
    dialog.appendChild(footer);

    overlay.appendChild(dialog);
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) { document.body.removeChild(overlay); resolve(null); }
    });

    // ── State ──
    let activeCategory = "全部";
    let searchText = "";

    function applyFilter() {
      const q = searchText.toLowerCase();
      return visibleTypes.filter((n) => {
        const catMatch = activeCategory === "全部" || n.category === activeCategory;
        if (!catMatch) return false;
        if (!q) return true;
        return (
          n.display_name.toLowerCase().includes(q) ||
          n.type_id.toLowerCase().includes(q) ||
          n.category.toLowerCase().includes(q) ||
          n.description.toLowerCase().includes(q)
        );
      });
    }

    function renderList() {
      listEl.innerHTML = "";
      const filtered = applyFilter();
      if (filtered.length === 0) {
        const empty = document.createElement("div");
        empty.className = "zh-an-empty";
        empty.textContent = "无匹配节点";
        listEl.appendChild(empty);
        return;
      }
      for (const nt of filtered) {
        const item = document.createElement("div");
        item.className = "zh-an-item";

        const top = document.createElement("div");
        top.className = "zh-an-item-top";

        const name = document.createElement("span");
        name.className = "zh-an-name";
        name.textContent = nt.display_name;
        top.appendChild(name);

        const badge = document.createElement("span");
        badge.className = "zh-an-badge";
        badge.textContent = nt.category;
        top.appendChild(badge);

        item.appendChild(top);

        if (nt.description) {
          const desc = document.createElement("div");
          desc.className = "zh-an-desc";
          desc.textContent = nt.description;
          item.appendChild(desc);
        }

        item.addEventListener("click", () => {
          document.body.removeChild(overlay);
          resolve(nt.type_id);
        });

        listEl.appendChild(item);
      }
    }

    function renderTabs() {
      tabsRow.innerHTML = "";
      for (const cat of cats) {
        const tab = document.createElement("button");
        tab.className = "zh-an-tab" + (cat === activeCategory ? " active" : "");
        tab.textContent = cat;
        tab.addEventListener("click", () => {
          activeCategory = cat;
          renderTabs();
          renderList();
        });
        tabsRow.appendChild(tab);
      }
    }

    searchInput.addEventListener("input", () => {
      searchText = searchInput.value;
      renderList();
    });

    renderTabs();
    renderList();

    document.body.appendChild(overlay);
    // Auto-focus the search field
    setTimeout(() => searchInput.focus(), 0);
  });
}

// ─── Save As dialog ───────────────────────────────────────────────────────────

/**
 * Ask the user where to save the current graph.
 * Resolves with "local" (download to disk), "workflow" (server workflow_set/), or null (cancelled).
 */
export function showSaveAsDialog(currentName: string): Promise<"local" | "workflow" | null> {
  return new Promise((resolve) => {
    const { dialog, close } = openOverlay();
    dialog.style.minWidth = "320px";
    dialog.style.maxWidth = "420px";

    const title = document.createElement("h3");
    title.textContent = "另存为";
    dialog.appendChild(title);

    const hint = document.createElement("p");
    hint.style.cssText = "margin:0 0 16px;font-size:13px;color:#aaa;";
    hint.textContent = `当前文件: ${currentName}`;
    dialog.appendChild(hint);

    const buttons = document.createElement("div");
    buttons.className = "zh-buttons";
    buttons.style.flexDirection = "column";
    buttons.style.gap = "8px";

    const localBtn = document.createElement("button");
    localBtn.textContent = "下载到本地";
    localBtn.style.width = "100%";
    localBtn.addEventListener("click", () => { close(); resolve("local"); });

    const workflowBtn = document.createElement("button");
    workflowBtn.textContent = "保存到工作流集";
    workflowBtn.style.width = "100%";
    workflowBtn.addEventListener("click", () => { close(); resolve("workflow"); });

    const cancelBtn = document.createElement("button");
    cancelBtn.textContent = "取消";
    cancelBtn.style.cssText = "width:100%;background:transparent;border-color:#555;color:#aaa;";
    cancelBtn.addEventListener("click", () => { close(); resolve(null); });

    buttons.appendChild(workflowBtn);
    buttons.appendChild(localBtn);
    buttons.appendChild(cancelBtn);
    dialog.appendChild(buttons);
  });
}

// ─── Workflow selection dialog ────────────────────────────────────────────────

/**
 * Show a modal listing workflow filenames. Resolves with the chosen filename
 * (e.g. "my_flow.json") or `null` if the user cancels.
 */
export function showWorkflowsDialog(files: string[]): Promise<string | null> {
  ensureDialogStyles();

  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "zh-overlay";

    const dialog = document.createElement("div");
    dialog.className = "zh-dialog";
    dialog.style.minWidth = "320px";
    dialog.style.maxWidth = "480px";

    const title = document.createElement("h3");
    title.textContent = "打开 Workflow";
    dialog.appendChild(title);

    const list = document.createElement("div");
    list.style.cssText = "display:flex;flex-direction:column;gap:4px;max-height:50vh;overflow-y:auto;margin-bottom:12px;";

    for (const file of files) {
      const item = document.createElement("div");
      item.style.cssText =
        "padding:8px 12px;border:1px solid #2a2a4a;border-radius:4px;cursor:pointer;transition:background 0.1s;font-size:13px;";
      item.textContent = file;
      item.addEventListener("mouseenter", () => { item.style.background = "#1a3a6e"; });
      item.addEventListener("mouseleave", () => { item.style.background = ""; });
      item.addEventListener("click", () => {
        document.body.removeChild(overlay);
        resolve(file);
      });
      list.appendChild(item);
    }

    dialog.appendChild(list);

    const buttons = document.createElement("div");
    buttons.className = "zh-buttons";

    const cancelBtn = document.createElement("button");
    cancelBtn.textContent = "取消";
    cancelBtn.addEventListener("click", () => {
      document.body.removeChild(overlay);
      resolve(null);
    });
    buttons.appendChild(cancelBtn);
    dialog.appendChild(buttons);

    overlay.appendChild(dialog);
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) {
        document.body.removeChild(overlay);
        resolve(null);
      }
    });

    document.body.appendChild(overlay);
  });
}

// ─── Workflow Browser Dialog ──────────────────────────────────────────────────

const BROWSER_STYLES = `
  .zh-wf-browser-overlay {
    position: fixed; inset: 0; background: rgba(0,0,0,0.7);
    display: flex; align-items: center; justify-content: center;
    z-index: 9999; font-family: sans-serif;
  }
  .zh-wf-browser-dialog {
    background: #1a1a2e; border: 1px solid #2a2a4a; border-radius: 10px;
    width: 860px; max-width: 95vw; max-height: 85vh;
    display: flex; flex-direction: column;
    box-shadow: 0 12px 40px rgba(0,0,0,0.6); color: #e0e0e0;
  }
  .zh-wf-browser-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 20px; border-bottom: 1px solid #2a2a4a; flex-shrink: 0;
  }
  .zh-wf-browser-header h3 { margin: 0; font-size: 16px; color: #8ab4f8; }
  .zh-wf-browser-close {
    background: transparent; border: none; color: #aaa; font-size: 22px;
    cursor: pointer; padding: 0 4px; line-height: 1;
  }
  .zh-wf-browser-close:hover { color: #e94560; }
  .zh-wf-browser-grid {
    flex: 1; overflow-y: auto; padding: 16px 20px;
    display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 14px; align-content: start;
  }
  .zh-wf-card {
    border: 1px solid #2a2a4a; border-radius: 8px; overflow: hidden;
    cursor: pointer; transition: border-color 0.15s, transform 0.1s, box-shadow 0.15s;
    background: #0d1117; display: flex; flex-direction: column;
  }
  .zh-wf-card:hover {
    border-color: #8ab4f8; transform: translateY(-2px);
    box-shadow: 0 6px 20px rgba(0,0,0,0.5);
  }
  .zh-wf-card-cover {
    width: 100%; aspect-ratio: 16/9; background: #0a0e1a;
    display: flex; align-items: center; justify-content: center;
    overflow: hidden; flex-shrink: 0;
  }
  .zh-wf-card-cover img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .zh-wf-card-cover .zh-wf-no-cover {
    font-size: 36px; opacity: 0.25; user-select: none; color: #8ab4f8;
  }
  .zh-wf-card-name {
    padding: 8px 10px; font-size: 12px; color: #cdd;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    border-top: 1px solid #1a2a3a; font-weight: 500;
  }
  .zh-wf-empty {
    grid-column: 1 / -1; color: #666; font-size: 14px;
    text-align: center; padding: 40px 0;
  }
`;

function ensureBrowserStyles(): void {
  if (document.getElementById("zh-wf-browser-styles")) return;
  const style = document.createElement("style");
  style.id = "zh-wf-browser-styles";
  style.textContent = BROWSER_STYLES;
  document.head.appendChild(style);
}

export interface WorkflowEntry {
  name: string;
  file: string;
  cover_url: string | null;
}

/** Show a card-grid browser for workflow_set entries. Returns the selected file name or null. */
export function showWorkflowBrowserDialog(workflows: WorkflowEntry[]): Promise<string | null> {
  ensureBrowserStyles();

  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "zh-wf-browser-overlay";

    const dialog = document.createElement("div");
    dialog.className = "zh-wf-browser-dialog";
    dialog.addEventListener("click", (e) => e.stopPropagation());

    // Header
    const header = document.createElement("div");
    header.className = "zh-wf-browser-header";
    const title = document.createElement("h3");
    title.textContent = "浏览工作流集";
    const closeBtn = document.createElement("button");
    closeBtn.className = "zh-wf-browser-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭";
    const close = () => { overlay.remove(); resolve(null); };
    closeBtn.addEventListener("click", close);
    header.appendChild(title);
    header.appendChild(closeBtn);
    dialog.appendChild(header);

    // Grid
    const grid = document.createElement("div");
    grid.className = "zh-wf-browser-grid";

    if (workflows.length === 0) {
      const empty = document.createElement("div");
      empty.className = "zh-wf-empty";
      empty.textContent = "workflow_set/ 目录中没有工作流文件";
      grid.appendChild(empty);
    } else {
      for (const wf of workflows) {
        const card = document.createElement("div");
        card.className = "zh-wf-card";
        card.title = wf.name;

        const coverDiv = document.createElement("div");
        coverDiv.className = "zh-wf-card-cover";

        if (wf.cover_url) {
          const img = document.createElement("img");
          img.src = wf.cover_url;
          img.alt = wf.name;
          img.draggable = false;
          coverDiv.appendChild(img);
        } else {
          const placeholder = document.createElement("span");
          placeholder.className = "zh-wf-no-cover";
          placeholder.textContent = "⬡";
          coverDiv.appendChild(placeholder);
        }

        const nameDiv = document.createElement("div");
        nameDiv.className = "zh-wf-card-name";
        nameDiv.textContent = wf.name;

        card.appendChild(coverDiv);
        card.appendChild(nameDiv);

        card.addEventListener("click", () => {
          overlay.remove();
          resolve(wf.file);
        });

        grid.appendChild(card);
      }
    }

    dialog.appendChild(grid);
    overlay.appendChild(dialog);

    overlay.addEventListener("click", close);
    document.body.appendChild(overlay);
  });
}

// ─── Hyperparameters Dialog ───────────────────────────────────────────────────

const HP_SCALAR_TYPES = ["String", "Integer", "Float", "Boolean"] as const;

export async function openHyperparametersDialog(
  sessionId: string,
  onSaved: () => void
): Promise<void> {
  ensureDialogStyles();
  const { dialog, close } = openOverlay();
  dialog.style.minWidth = "760px";

  const title = document.createElement("h3");
  title.textContent = "超参数管理";
  dialog.appendChild(title);

  const hint = document.createElement("div");
  hint.className = "zh-hint";
  hint.textContent = "超参数在磁盘上共享，运行前注入已绑定的节点输入端口。";
  dialog.appendChild(hint);

  const sectionLabel = document.createElement("div");
  sectionLabel.className = "zh-section-label";
  sectionLabel.textContent = "超参数列表";
  dialog.appendChild(sectionLabel);

  const listContainer = document.createElement("div");
  dialog.appendChild(listContainer);

  let hpResp: { hyperparameters: HyperParameter[]; values: Record<string, unknown> };
  try {
    hpResp = await graphs.getHyperparameters(sessionId);
  } catch (e) {
    alert("加载超参数失败: " + (e as Error).message);
    close();
    return;
  }

  type HpRow = {
    nameEl: HTMLInputElement;
    typeEl: HTMLSelectElement;
    groupEl: HTMLInputElement;
    requiredEl: HTMLInputElement;
    descEl: HTMLInputElement;
    valueEl: HTMLInputElement;
  };
  const rows: HpRow[] = [];

  const COL = "1fr 100px 100px 50px 1fr 1fr 32px";

  // Header row
  const headerRow = document.createElement("div");
  headerRow.style.cssText = `display:grid;grid-template-columns:${COL};gap:6px;margin-bottom:4px;font-size:11px;color:#888;padding:0 2px;`;
  for (const h of ["名称", "类型", "分组", "必填", "描述", "当前值", ""]) {
    const span = document.createElement("span");
    span.textContent = h;
    headerRow.appendChild(span);
  }
  listContainer.appendChild(headerRow);

  const rowsContainer = document.createElement("div");
  listContainer.appendChild(rowsContainer);

  const addRow = (hp?: HyperParameter) => {
    const currentValue = hp ? (hpResp.values[hp.name] ?? "") : "";
    const row = document.createElement("div");
    row.style.cssText = `display:grid;grid-template-columns:${COL};gap:6px;align-items:center;margin-bottom:6px;`;

    const makeInput = (placeholder: string, value: string): HTMLInputElement => {
      const el = document.createElement("input");
      el.type = "text";
      el.placeholder = placeholder;
      el.value = value;
      el.style.marginBottom = "0";
      return el;
    };

    const nameEl = makeInput("名称", hp?.name ?? "");

    const typeEl = document.createElement("select");
    typeEl.style.marginBottom = "0";
    for (const t of HP_SCALAR_TYPES) {
      const opt = document.createElement("option");
      opt.value = t;
      opt.textContent = t;
      if (t === (hp?.data_type ?? "String")) opt.selected = true;
      typeEl.appendChild(opt);
    }

    const groupEl = makeInput("分组", hp?.group ?? "default");
    const descEl = makeInput("描述", hp?.description ?? "");
    const valueEl = makeInput("当前值", currentValue !== "" ? String(currentValue) : "");

    const requiredWrap = document.createElement("label");
    requiredWrap.style.cssText = "display:flex;align-items:center;justify-content:center;gap:4px;font-size:12px;cursor:pointer;";
    const requiredEl = document.createElement("input");
    requiredEl.type = "checkbox";
    requiredEl.checked = hp?.required ?? false;
    requiredWrap.appendChild(requiredEl);
    requiredWrap.appendChild(document.createTextNode("是"));

    const removeBtn = document.createElement("button");
    removeBtn.textContent = "✕";
    removeBtn.className = "danger";
    removeBtn.style.cssText = "padding:2px 6px;font-size:12px;";
    removeBtn.addEventListener("click", () => {
      const i = rows.findIndex((r) => r.nameEl === nameEl);
      if (i >= 0) rows.splice(i, 1);
      row.remove();
    });

    row.appendChild(nameEl);
    row.appendChild(typeEl);
    row.appendChild(groupEl);
    row.appendChild(requiredWrap);
    row.appendChild(descEl);
    row.appendChild(valueEl);
    row.appendChild(removeBtn);
    rowsContainer.appendChild(row);

    rows.push({ nameEl, typeEl, groupEl, requiredEl, descEl, valueEl });
  };

  for (const hp of hpResp.hyperparameters) addRow(hp);

  const addBtn = document.createElement("button");
  addBtn.textContent = "+ 添加超参数";
  addBtn.style.marginTop = "6px";
  addBtn.addEventListener("click", () => addRow());
  listContainer.appendChild(addBtn);

  const btns = document.createElement("div");
  btns.className = "zh-buttons";

  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  cancelBtn.addEventListener("click", close);

  const saveBtn = document.createElement("button");
  saveBtn.textContent = "保存";
  saveBtn.className = "primary";
  saveBtn.addEventListener("click", async () => {
    const newHPs: HyperParameter[] = rows
      .filter((r) => r.nameEl.value.trim())
      .map((r) => ({
        name: r.nameEl.value.trim(),
        data_type: r.typeEl.value,
        group: r.groupEl.value.trim() || "default",
        required: r.requiredEl.checked,
        description: r.descEl.value.trim() || null,
      }));

    const valuesMap: Record<string, unknown> = {};
    for (const r of rows) {
      const name = r.nameEl.value.trim();
      if (!name) continue;
      const raw = r.valueEl.value;
      switch (r.typeEl.value) {
        case "Integer": valuesMap[name] = parseInt(raw, 10) || 0; break;
        case "Float": valuesMap[name] = parseFloat(raw) || 0.0; break;
        case "Boolean": valuesMap[name] = raw === "true" || raw === "1" || raw === "yes"; break;
        default: valuesMap[name] = raw;
      }
    }

    try {
      const graphDef = await graphs.get(sessionId);
      const groups = [...new Set(newHPs.map((h) => h.group))];
      await graphs.put(sessionId, {
        ...graphDef,
        hyperparameters: newHPs,
        hyperparameter_groups: groups,
      });
      await graphs.updateHyperparameters(sessionId, valuesMap);
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

// ─── Variables Dialog ─────────────────────────────────────────────────────────

export async function openVariablesDialog(
  sessionId: string,
  onSaved: () => void
): Promise<void> {
  ensureDialogStyles();
  const { dialog, close } = openOverlay();
  dialog.style.minWidth = "560px";

  const title = document.createElement("h3");
  title.textContent = "变量管理";
  dialog.appendChild(title);

  const hint = document.createElement("div");
  hint.className = "zh-hint";
  hint.textContent = "变量存储于节点图中，运行期间可在节点间读写，通过端口绑定注入。";
  dialog.appendChild(hint);

  const sectionLabel = document.createElement("div");
  sectionLabel.className = "zh-section-label";
  sectionLabel.textContent = "变量列表";
  dialog.appendChild(sectionLabel);

  const listContainer = document.createElement("div");
  dialog.appendChild(listContainer);

  let variables: GraphVariable[];
  try {
    variables = await graphs.getVariables(sessionId);
  } catch (e) {
    alert("加载变量失败: " + (e as Error).message);
    close();
    return;
  }

  type VarRow = { nameEl: HTMLInputElement; typeEl: HTMLSelectElement; valueEl: HTMLInputElement };
  const rows: VarRow[] = [];

  const COL = "1fr 120px 1fr 32px";

  const headerRow = document.createElement("div");
  headerRow.style.cssText = `display:grid;grid-template-columns:${COL};gap:6px;margin-bottom:4px;font-size:11px;color:#888;padding:0 2px;`;
  for (const h of ["名称", "类型", "初始值 (JSON)", ""]) {
    const span = document.createElement("span");
    span.textContent = h;
    headerRow.appendChild(span);
  }
  listContainer.appendChild(headerRow);

  const rowsContainer = document.createElement("div");
  listContainer.appendChild(rowsContainer);

  const addRow = (v?: GraphVariable) => {
    const row = document.createElement("div");
    row.style.cssText = `display:grid;grid-template-columns:${COL};gap:6px;align-items:center;margin-bottom:6px;`;

    const nameEl = document.createElement("input");
    nameEl.type = "text";
    nameEl.placeholder = "名称";
    nameEl.value = v?.name ?? "";
    nameEl.style.marginBottom = "0";

    const typeEl = dataTypeSelect(v?.data_type ?? "String");
    typeEl.style.marginBottom = "0";

    const valueEl = document.createElement("input");
    valueEl.type = "text";
    valueEl.placeholder = "初始值 (JSON)";
    valueEl.value = v?.initial_value != null ? JSON.stringify(v.initial_value) : "";
    valueEl.style.marginBottom = "0";

    const removeBtn = document.createElement("button");
    removeBtn.textContent = "✕";
    removeBtn.className = "danger";
    removeBtn.style.cssText = "padding:2px 6px;font-size:12px;";
    removeBtn.addEventListener("click", () => {
      const i = rows.findIndex((r) => r.nameEl === nameEl);
      if (i >= 0) rows.splice(i, 1);
      row.remove();
    });

    row.appendChild(nameEl);
    row.appendChild(typeEl);
    row.appendChild(valueEl);
    row.appendChild(removeBtn);
    rowsContainer.appendChild(row);

    rows.push({ nameEl, typeEl, valueEl });
  };

  for (const v of variables) addRow(v);

  const addBtn = document.createElement("button");
  addBtn.textContent = "+ 添加变量";
  addBtn.style.marginTop = "6px";
  addBtn.addEventListener("click", () => addRow());
  listContainer.appendChild(addBtn);

  const btns = document.createElement("div");
  btns.className = "zh-buttons";

  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  cancelBtn.addEventListener("click", close);

  const saveBtn = document.createElement("button");
  saveBtn.textContent = "保存";
  saveBtn.className = "primary";
  saveBtn.addEventListener("click", async () => {
    const newVars: GraphVariable[] = rows
      .filter((r) => r.nameEl.value.trim())
      .map((r) => {
        let initVal: unknown = null;
        const raw = r.valueEl.value.trim();
        if (raw) {
          try { initVal = JSON.parse(raw); } catch { initVal = raw; }
        }
        return { name: r.nameEl.value.trim(), data_type: r.typeEl.value, initial_value: initVal };
      });

    try {
      await graphs.updateVariables(sessionId, newVars);
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
