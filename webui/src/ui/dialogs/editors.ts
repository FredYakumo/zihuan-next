import { graphs, fileIO } from "../../api/client";
import type { NodeDefinition, Port } from "../../api/types";
import { openOverlay } from "./base";
import { buildPortListEditor, escapeHtml, extractTemplateVars } from "./shared";
import { cloneDataTypeMetaData, dataTypeSelect, parseDisplayDataType } from "./data_types";
import { ensureToolSubgraphSignature } from "./tool_subgraph_utils";
import type { DataTypeMetaData } from "../../api/types";
import type {
  BrainToolDefinition,
  EmbeddedFunctionConfig,
  FunctionPortDef,
  OpenAIMessageItem,
  QQMessageItem,
  ToolParamDef,
} from "./types";

interface JsonFieldDef {
  name: string;
  data_type: string;
}

export function openFormatStringEditor(
  nodeDef: NodeDefinition,
  sessionId: string,
  onSaved: () => void,
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
    if (openIdx < 0) {
      hideAutoComplete();
      return;
    }

    const afterOpen = before.substring(openIdx + 2);
    if (afterOpen.includes("}")) {
      hideAutoComplete();
      return;
    }

    const prefix = afterOpen.toLowerCase();
    const matches = currentVars.filter((v) => v.toLowerCase().startsWith(prefix));
    if (matches.length === 0) {
      hideAutoComplete();
      return;
    }

    autocomplete.innerHTML = "";
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
        e.preventDefault();
        const full = ta.value;
        const newText = full.substring(0, openIdx) + `\${${v}}` + full.substring(cursor);
        ta.value = newText;
        ta.selectionStart = ta.selectionEnd = openIdx + v.length + 3;
        hideAutoComplete();
        ta.dispatchEvent(new Event("input"));
        ta.focus();
      });
      item.addEventListener("mouseenter", () => {
        item.style.background = "#1a3a6e";
      });
      item.addEventListener("mouseleave", () => {
        item.style.background = "";
      });
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
      const nodeIdx = graphDef.nodes.findIndex((n) => n.id === nodeDef.id);
      if (nodeIdx < 0) {
        alert("Node not found");
        close();
        return;
      }

      const currentNode = graphDef.nodes[nodeIdx];
      const staticPorts = currentNode.input_ports.filter((p) => p.name === "template");
      const newDynamicPorts: Port[] = varNames.map((name) => ({
        name,
        data_type: "Any",
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

export function openJsonExtractEditor(
  nodeDef: NodeDefinition,
  sessionId: string,
  onSaved: () => void,
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
      const i = items.findIndex((it) => it.nameEl === nameEl);
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
      .map((it) => ({ name: it.nameEl.value.trim(), data_type: it.typeEl.value }))
      .filter((f) => f.name);

    try {
      const graphDef = await graphs.get(sessionId);
      const nodeIdx = graphDef.nodes.findIndex((n) => n.id === nodeDef.id);
      if (nodeIdx < 0) {
        alert("Node not found");
        close();
        return;
      }

      const newOutputPorts: Port[] = fields.map((f) => ({
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

export function openFunctionSignatureEditor(
  nodeDef: NodeDefinition,
  sessionId: string,
  onSaved: () => void,
  onEditSubgraph: (config: EmbeddedFunctionConfig) => void,
): void {
  const { dialog, close } = openOverlay();

  const cfg: EmbeddedFunctionConfig = {
    name: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.name) ?? nodeDef.name,
    description: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.description) ?? "",
    inputs: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.inputs) ?? [],
    outputs: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.outputs) ?? [],
    subgraph: ((nodeDef.inline_values?.["function_config"] as EmbeddedFunctionConfig | undefined)?.subgraph) ?? {
      nodes: [], edges: [], hyperparameter_groups: [], hyperparameters: [], variables: [],
    } as any,
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
        name: updatedConfig.name,
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

export function openBrainToolsEditor(
  nodeDef: NodeDefinition,
  sessionId: string,
  onSaved: () => void,
  onEditToolSubgraph: (toolIndex: number, tool: BrainToolDefinition) => void,
): void {
  const { dialog, close } = openOverlay();
  const isQqMessageAgent =
    nodeDef.node_type === "qq_chat_agent" || nodeDef.node_type === "qq_message_agent";
  const ownerLabel = isQqMessageAgent ? "QQ Message Agent" : "Brain";

  const rawTools = (nodeDef.inline_values?.["tools_config"] as BrainToolDefinition[] | undefined) ?? [];
  const rawSharedInputs = (nodeDef.inline_values?.["shared_inputs"] as FunctionPortDef[] | undefined) ?? [];

  const tools: BrainToolDefinition[] = JSON.parse(JSON.stringify(rawTools));
  const sharedInputs: FunctionPortDef[] = JSON.parse(JSON.stringify(rawSharedInputs));
  let readSharedInputs = (): FunctionPortDef[] => JSON.parse(JSON.stringify(sharedInputs));

  const render = () => {
    dialog.innerHTML = `<h3>管理 ${ownerLabel} 工具</h3>
      <div class="zh-section-label">共享输入端口 (shared_inputs)</div>
    `;

    const sharedContainer = document.createElement("div");
    const getSharedInputs = buildPortListEditor(sharedContainer, sharedInputs);
    readSharedInputs = getSharedInputs;
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
      tools.push(ensureToolSubgraphSignature(nodeDef.node_type, readSharedInputs(), {
        id: `tool_${Date.now()}`,
        name: `tool_${tools.length + 1}`,
        description: "",
        parameters: [],
        outputs: isQqMessageAgent ? [{ name: "result", data_type: "String" }] : [],
        subgraph: {
          nodes: [],
          edges: [],
          hyperparameter_groups: [],
          hyperparameters: [],
          variables: [],
          metadata: { name: null, description: null, version: null },
        } as any,
      }));
      close();
      openBrainToolsEditor(
        { ...nodeDef, inline_values: { ...nodeDef.inline_values, tools_config: tools, shared_inputs: readSharedInputs() } },
        sessionId,
        onSaved,
        onEditToolSubgraph,
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
        const sharedInputsToSave = readSharedInputs();
        const toolsToSave = tools.map((tool) =>
          ensureToolSubgraphSignature(nodeDef.node_type, sharedInputsToSave, tool)
        );
        await graphs.updateNode(sessionId, nodeDef.id, {
          inline_values: {
            tools_config: toolsToSave as unknown as unknown[],
            shared_inputs: sharedInputsToSave as unknown as unknown[],
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

    const headerRow = document.createElement("div");
    headerRow.style.cssText = "display:flex;align-items:center;gap:8px;margin-bottom:8px;";

    const nameInput = document.createElement("input");
    nameInput.type = "text";
    nameInput.value = tool.name;
    nameInput.placeholder = "工具名";
    nameInput.style.cssText = "flex:1;margin-bottom:0;";
    nameInput.addEventListener("change", () => {
      tools[idx].name = nameInput.value.trim();
    });

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

    const descLabel = document.createElement("label");
    descLabel.textContent = "描述";
    const descInput = document.createElement("input");
    descInput.type = "text";
    descInput.value = tool.description;
    descInput.placeholder = "工具描述";
    descInput.addEventListener("change", () => {
      tools[idx].description = descInput.value.trim();
    });
    card.appendChild(descLabel);
    card.appendChild(descInput);

    const paramLabel = document.createElement("div");
    paramLabel.className = "zh-section-label";
    paramLabel.textContent = "参数 (parameters)";
    card.appendChild(paramLabel);

    const paramContainer = document.createElement("div");
    const paramItems: Array<{
      nameEl: HTMLInputElement;
      typeEl: HTMLSelectElement;
      descEl: HTMLInputElement;
      dataType: DataTypeMetaData;
    }> = [];

    const syncParams = () => {
      tools[idx].parameters = paramItems
        .map((it) => ({
          name: it.nameEl.value.trim(),
          data_type: cloneDataTypeMetaData(it.dataType),
          desc: it.descEl.value.trim(),
        }))
        .filter((p) => p.name);
    };

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
        const i = paramItems.findIndex((it) => it.nameEl === nameEl);
        if (i >= 0) paramItems.splice(i, 1);
        row.remove();
        syncParams();
      });

      row.appendChild(nameEl);
      row.appendChild(typeEl);
      row.appendChild(descEl);
      row.appendChild(removeBtn);
      paramContainer.insertBefore(row, addParamBtn);

      const item = {
        nameEl,
        typeEl,
        descEl,
        dataType: cloneDataTypeMetaData(param?.data_type ?? "String"),
      };
      typeEl.addEventListener("change", () => {
        item.dataType = parseDisplayDataType(typeEl.value);
        syncParams();
      });
      paramItems.push(item);
      [nameEl, descEl].forEach((el) => el.addEventListener("change", syncParams));
    };

    const addParamBtn = document.createElement("button");
    addParamBtn.textContent = "+ 添加参数";
    addParamBtn.style.marginBottom = "8px";
    addParamBtn.addEventListener("click", () => addParamRow());
    paramContainer.appendChild(addParamBtn);

    for (const p of tool.parameters) addParamRow(p);
    card.appendChild(paramContainer);

    const outLabel = document.createElement("div");
    outLabel.className = "zh-section-label";
    outLabel.textContent = "输出 (outputs)";
    card.appendChild(outLabel);

    const outContainer = document.createElement("div");
    const fixedStringOutput = [{ name: "result", data_type: "String" as DataTypeMetaData }];
    const getOutputs = isQqMessageAgent
      ? (() => fixedStringOutput.map((output) => ({ ...output })))
      : buildPortListEditor(outContainer, tool.outputs);
    if (isQqMessageAgent) {
      outContainer.innerHTML = `<div class="zh-hint">该节点的工具子图返回值固定为单个 <code>String</code> 输出：<code>result</code></div>`;
      tools[idx].outputs = getOutputs();
    } else {
      outContainer.addEventListener("input", () => {
        tools[idx].outputs = getOutputs();
      });
    }
    card.appendChild(outContainer);

    const editSubBtn = document.createElement("button");
    editSubBtn.textContent = "↳ 编辑工具子图";
    editSubBtn.style.marginTop = "6px";
    editSubBtn.addEventListener("click", () => {
      tools[idx].name = nameInput.value.trim();
      tools[idx].description = descInput.value.trim();
      syncParams();
      tools[idx].outputs = getOutputs();
      tools[idx] = ensureToolSubgraphSignature(nodeDef.node_type, readSharedInputs(), tools[idx]);
      close();
      onEditToolSubgraph(idx, tools[idx]);
    });
    card.appendChild(editSubBtn);

    return card;
  };

  render();
}

export function openQQMessageListEditor(
  nodeDef: NodeDefinition,
  sessionId: string,
  onSaved: () => void,
): void {
  const { dialog, close } = openOverlay();

  const rawMessages = (nodeDef.inline_values?.["messages"] as QQMessageItem[] | undefined) ?? [];
  const messages: QQMessageItem[] = JSON.parse(JSON.stringify(rawMessages));

  const render = () => {
    dialog.innerHTML = `<h3>编辑 QQ 消息列表</h3>`;

    const listLabel = document.createElement("div");
    listLabel.className = "zh-section-label";
    listLabel.textContent = `消息列表 (${messages.length})`;
    dialog.appendChild(listLabel);

    const listContainer = document.createElement("div");
    messages.forEach((msg, idx) => {
      listContainer.appendChild(buildMessageCard(msg, idx));
    });
    dialog.appendChild(listContainer);

    const addBtn = document.createElement("button");
    addBtn.textContent = "+ 添加消息";
    addBtn.style.marginBottom = "12px";
    addBtn.addEventListener("click", () => {
      messages.push({ type: "text", data: { text: "" } });
      close();
      openQQMessageListEditor(
        { ...nodeDef, inline_values: { ...nodeDef.inline_values, messages } },
        sessionId,
        onSaved,
      );
    });
    dialog.appendChild(addBtn);

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
            messages: messages as unknown as unknown[],
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

  const buildMessageCard = (msg: QQMessageItem, idx: number): HTMLElement => {
    const card = document.createElement("div");
    card.className = "zh-tool-card";

    const headerRow = document.createElement("div");
    headerRow.style.cssText = "display:flex;align-items:center;gap:8px;margin-bottom:8px;";

    const typeSelect = document.createElement("select");
    (["text", "at", "reply", "image"] as const).forEach((t) => {
      const opt = document.createElement("option");
      opt.value = t;
      opt.textContent =
        t === "text" ? "文本" : t === "at" ? "@提及" : t === "reply" ? "回复" : "图像";
      if (t === msg.type) opt.selected = true;
      typeSelect.appendChild(opt);
    });
    typeSelect.style.flex = "1";
    typeSelect.style.minWidth = "100px";
    typeSelect.addEventListener("change", () => {
      const newType = typeSelect.value as "text" | "at" | "reply" | "image";
      if (newType === "text") {
        messages[idx] = { type: "text", data: { text: "" } };
      } else if (newType === "at") {
        messages[idx] = { type: "at", data: { target: "" } };
      } else if (newType === "reply") {
        messages[idx] = { type: "reply", data: { id: 0 } };
      } else {
        messages[idx] = { type: "image", data: { url: "" } };
      }
      close();
      openQQMessageListEditor(
        { ...nodeDef, inline_values: { ...nodeDef.inline_values, messages } },
        sessionId,
        onSaved,
      );
    });

    const deleteBtn = document.createElement("button");
    deleteBtn.textContent = "✕ 删除";
    deleteBtn.className = "danger";
    deleteBtn.style.cssText = "padding:4px 8px;white-space:nowrap;flex-shrink:0;";
    deleteBtn.addEventListener("click", () => {
      messages.splice(idx, 1);
      close();
      openQQMessageListEditor(
        { ...nodeDef, inline_values: { ...nodeDef.inline_values, messages } },
        sessionId,
        onSaved,
      );
    });

    headerRow.appendChild(typeSelect);
    headerRow.appendChild(deleteBtn);
    card.appendChild(headerRow);

    if (msg.type === "text") {
      const label = document.createElement("label");
      label.textContent = "文本内容";
      const input = document.createElement("input");
      input.type = "text";
      input.value = msg.data.text ?? "";
      input.placeholder = "输入消息文本";
      input.addEventListener("input", () => {
        messages[idx].data.text = input.value;
      });
      card.appendChild(label);
      card.appendChild(input);
    } else if (msg.type === "at") {
      const label = document.createElement("label");
      label.textContent = "目标 QQ 号";
      const input = document.createElement("input");
      input.type = "text";
      input.value = msg.data.target ?? "";
      input.placeholder = "QQ 号 (如: 123456)";
      input.addEventListener("input", () => {
        messages[idx].data.target = input.value;
      });
      card.appendChild(label);
      card.appendChild(input);
    } else if (msg.type === "reply") {
      const label = document.createElement("label");
      label.textContent = "回复消息 ID";
      const input = document.createElement("input");
      input.type = "number";
      input.value = String(msg.data.id ?? 0);
      input.placeholder = "消息 ID";
      input.addEventListener("input", () => {
        const val = parseInt(input.value, 10);
        messages[idx].data.id = isNaN(val) ? 0 : val;
      });
      card.appendChild(label);
      card.appendChild(input);
    } else if (msg.type === "image") {
      buildImageMessageInputs(card, msg, idx);
    }

    return card;
  };

  const buildImageMessageInputs = (
    card: HTMLElement,
    msg: QQMessageItem,
    idx: number,
  ): void => {
    const previewWrap = document.createElement("div");
    previewWrap.style.cssText = "margin-bottom:8px;";
    const renderPreview = () => {
      previewWrap.innerHTML = "";
      const url = msg.data.url ?? "";
      if (!url) return;
      const img = document.createElement("img");
      img.src = url;
      img.alt = msg.data.name ?? "image";
      img.style.cssText =
        "max-width:200px;max-height:200px;border-radius:4px;display:block;";
      img.addEventListener("error", () => {
        img.style.opacity = "0.4";
        img.title = "图像加载失败";
      });
      previewWrap.appendChild(img);
    };
    renderPreview();
    card.appendChild(previewWrap);

    const dropZone = document.createElement("div");
    dropZone.textContent = "拖拽图像到此处或点击上传";
    dropZone.style.cssText =
      "border:1px dashed #888;border-radius:4px;padding:16px;text-align:center;cursor:pointer;color:#aaa;margin-bottom:8px;user-select:none;";

    const fileInput = document.createElement("input");
    fileInput.type = "file";
    fileInput.accept = "image/*";
    fileInput.style.display = "none";

    const status = document.createElement("div");
    status.style.cssText = "font-size:12px;color:#888;margin-bottom:8px;min-height:16px;";

    const urlLabel = document.createElement("label");
    urlLabel.textContent = "图像 URL";
    const urlInput = document.createElement("input");
    urlInput.type = "text";
    urlInput.value = msg.data.url ?? "";
    urlInput.placeholder = "https://… 或上传后自动填入";
    urlInput.addEventListener("input", () => {
      messages[idx].data.url = urlInput.value;
      msg.data.url = urlInput.value;
      renderPreview();
    });

    const summaryLabel = document.createElement("label");
    summaryLabel.textContent = "摘要 (可选)";
    const summaryInput = document.createElement("input");
    summaryInput.type = "text";
    summaryInput.value = msg.data.summary ?? "";
    summaryInput.placeholder = "图像描述文字";
    summaryInput.addEventListener("input", () => {
      messages[idx].data.summary = summaryInput.value;
    });

    const handleFile = async (file: File) => {
      if (!file.type.startsWith("image/")) {
        status.textContent = `不支持的文件类型: ${file.type || "未知"}`;
        status.style.color = "#e57373";
        return;
      }
      status.textContent = `上传中… (${file.name})`;
      status.style.color = "#888";
      try {
        const result = await fileIO.uploadImage(file);
        messages[idx].data.url = result.url;
        messages[idx].data.name = result.name;
        msg.data.url = result.url;
        msg.data.name = result.name;
        urlInput.value = result.url;
        renderPreview();
        status.textContent = `已上传: ${result.name}`;
        status.style.color = "#81c784";
      } catch (e) {
        status.textContent = "上传失败: " + (e as Error).message;
        status.style.color = "#e57373";
      }
    };

    dropZone.addEventListener("click", () => fileInput.click());
    fileInput.addEventListener("change", () => {
      const file = fileInput.files?.[0];
      if (file) {
        handleFile(file);
        fileInput.value = "";
      }
    });
    dropZone.addEventListener("dragover", (e) => {
      e.preventDefault();
      dropZone.style.borderColor = "#4fc3f7";
      dropZone.style.color = "#4fc3f7";
    });
    dropZone.addEventListener("dragleave", () => {
      dropZone.style.borderColor = "#888";
      dropZone.style.color = "#aaa";
    });
    dropZone.addEventListener("drop", (e) => {
      e.preventDefault();
      dropZone.style.borderColor = "#888";
      dropZone.style.color = "#aaa";
      const file = e.dataTransfer?.files?.[0];
      if (file) handleFile(file);
    });

    card.appendChild(dropZone);
    card.appendChild(fileInput);
    card.appendChild(status);
    card.appendChild(urlLabel);
    card.appendChild(urlInput);
    card.appendChild(summaryLabel);
    card.appendChild(summaryInput);
  };

  render();
}

export function openOpenAIMessageListEditor(
  nodeDef: NodeDefinition,
  sessionId: string,
  onSaved: () => void,
): void {
  const { dialog, close } = openOverlay();

  const rawMessages = (nodeDef.inline_values?.["messages"] as OpenAIMessageItem[] | undefined) ?? [];
  const messages: OpenAIMessageItem[] = JSON.parse(JSON.stringify(rawMessages));

  const render = () => {
    dialog.innerHTML = `<h3>编辑 OpenAI 消息列表</h3>`;

    const listLabel = document.createElement("div");
    listLabel.className = "zh-section-label";
    listLabel.textContent = `消息列表 (${messages.length})`;
    dialog.appendChild(listLabel);

    const hint = document.createElement("div");
    hint.className = "zh-hint";
    hint.innerHTML = "支持编辑 <code>role</code>、<code>content</code>，以及 tool 消息的 <code>tool_call_id</code>。";
    dialog.appendChild(hint);

    const listContainer = document.createElement("div");
    messages.forEach((msg, idx) => {
      listContainer.appendChild(buildMessageCard(msg, idx));
    });
    dialog.appendChild(listContainer);

    const addBtn = document.createElement("button");
    addBtn.textContent = "+ 添加消息";
    addBtn.style.marginBottom = "12px";
    addBtn.addEventListener("click", () => {
      messages.push({ role: "user", content: "" });
      close();
      openOpenAIMessageListEditor(
        { ...nodeDef, inline_values: { ...nodeDef.inline_values, messages } },
        sessionId,
        onSaved,
      );
    });
    dialog.appendChild(addBtn);

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
            messages: messages as unknown as unknown[],
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

  const buildMessageCard = (msg: OpenAIMessageItem, idx: number): HTMLElement => {
    const card = document.createElement("div");
    card.className = "zh-tool-card";

    const headerRow = document.createElement("div");
    headerRow.style.cssText = "display:flex;align-items:center;gap:8px;margin-bottom:8px;";

    const roleSelect = document.createElement("select");
    (["system", "user", "assistant", "tool"] as const).forEach((role) => {
      const opt = document.createElement("option");
      opt.value = role;
      opt.textContent =
        role === "system" ? "system" : role === "user" ? "user" : role === "assistant" ? "assistant" : "tool";
      if ((msg.role ?? "user") === role) opt.selected = true;
      roleSelect.appendChild(opt);
    });
    roleSelect.style.flex = "1";
    roleSelect.style.minWidth = "120px";
    roleSelect.addEventListener("change", () => {
      messages[idx].role = roleSelect.value as OpenAIMessageItem["role"];
      if (messages[idx].role !== "tool") {
        delete messages[idx].tool_call_id;
      }
    });

    const deleteBtn = document.createElement("button");
    deleteBtn.textContent = "✕ 删除";
    deleteBtn.className = "danger";
    deleteBtn.style.cssText = "padding:4px 8px;white-space:nowrap;flex-shrink:0;";
    deleteBtn.addEventListener("click", () => {
      messages.splice(idx, 1);
      close();
      openOpenAIMessageListEditor(
        { ...nodeDef, inline_values: { ...nodeDef.inline_values, messages } },
        sessionId,
        onSaved,
      );
    });

    headerRow.appendChild(roleSelect);
    headerRow.appendChild(deleteBtn);
    card.appendChild(headerRow);

    const contentLabel = document.createElement("label");
    contentLabel.textContent = "内容";
    const contentInput = document.createElement("textarea");
    contentInput.value = msg.content ?? "";
    contentInput.placeholder = "输入消息内容";
    contentInput.style.minHeight = "96px";
    contentInput.addEventListener("input", () => {
      messages[idx].content = contentInput.value;
    });
    card.appendChild(contentLabel);
    card.appendChild(contentInput);

    const reasoningLabel = document.createElement("label");
    reasoningLabel.textContent = "推理内容（可选）";
    const reasoningInput = document.createElement("textarea");
    reasoningInput.value = msg.reasoning_content ?? "";
    reasoningInput.placeholder = "可选：reasoning_content";
    reasoningInput.style.minHeight = "72px";
    reasoningInput.addEventListener("input", () => {
      const value = reasoningInput.value.trim();
      messages[idx].reasoning_content = value ? value : null;
    });
    card.appendChild(reasoningLabel);
    card.appendChild(reasoningInput);

    if ((msg.role ?? "user") === "tool") {
      const toolCallIdLabel = document.createElement("label");
      toolCallIdLabel.textContent = "tool_call_id";
      const toolCallIdInput = document.createElement("input");
      toolCallIdInput.type = "text";
      toolCallIdInput.value = msg.tool_call_id ?? "";
      toolCallIdInput.placeholder = "输入 tool_call_id";
      toolCallIdInput.addEventListener("input", () => {
        const value = toolCallIdInput.value.trim();
        messages[idx].tool_call_id = value ? value : null;
      });
      card.appendChild(toolCallIdLabel);
      card.appendChild(toolCallIdInput);
    }

    return card;
  };

  render();
}
