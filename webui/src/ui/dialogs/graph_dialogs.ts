import { graphs, system, type ConnectionConfig } from "../../api/client";
import type { GraphMetadata, GraphVariable, HyperParameter } from "../../api/types";
import { ensureDialogStyles, openOverlay, showErrorDialog } from "./base";
import { dataTypeSelect } from "./data_types";

export const HP_TYPES = [
  "String",
  "Integer",
  "Float",
  "Boolean",
  "Password",
  "MySqlRef",
  "WeaviateRef",
  "RedisRef",
  "S3Ref",
  "BotAdapterRef",
  "TavilyRef",
] as const;

const HP_SCALAR_TYPES = HP_TYPES;

const CONNECTION_KIND_BY_HP_TYPE: Partial<Record<(typeof HP_TYPES)[number], ConnectionConfig["kind"]["type"]>> = {
  MySqlRef: "mysql",
  WeaviateRef: "weaviate",
  RedisRef: "redis",
  S3Ref: "rustfs",
  BotAdapterRef: "ims_bot_adapter",
  TavilyRef: "tavily",
};

function isConnectionHyperparameterType(type: string): boolean {
  return Object.prototype.hasOwnProperty.call(CONNECTION_KIND_BY_HP_TYPE, type);
}

export async function openGraphMetadataDialog(
  sessionId: string,
  onSaved: () => void,
): Promise<void> {
  ensureDialogStyles();
  const { dialog, close } = openOverlay();
  dialog.style.minWidth = "480px";
  dialog.style.maxWidth = "600px";

  const title = document.createElement("h3");
  title.textContent = "编辑节点图信息";
  dialog.appendChild(title);

  let current: GraphMetadata = { name: null, description: null, version: null };
  try {
    current = await graphs.getMetadata(sessionId);
  } catch {
    // keep defaults
  }

  const mk = (labelText: string, id: string) => {
    const lbl = document.createElement("label");
    lbl.htmlFor = id;
    lbl.textContent = labelText;
    dialog.appendChild(lbl);
  };

  mk("节点图名称", "meta-name");
  const nameEl = document.createElement("input");
  nameEl.id = "meta-name";
  nameEl.type = "text";
  nameEl.placeholder = "未命名";
  nameEl.value = current.name ?? "";
  dialog.appendChild(nameEl);

  mk("版本", "meta-version");
  const versionEl = document.createElement("input");
  versionEl.id = "meta-version";
  versionEl.type = "text";
  versionEl.placeholder = "1.0.0";
  versionEl.value = current.version ?? "";
  dialog.appendChild(versionEl);

  mk("描述", "meta-desc");
  const descEl = document.createElement("textarea");
  descEl.id = "meta-desc";
  descEl.placeholder = "描述这个节点图的功能…";
  descEl.style.minHeight = "100px";
  descEl.value = current.description ?? "";
  dialog.appendChild(descEl);

  const btns = document.createElement("div");
  btns.className = "zh-buttons";

  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  cancelBtn.addEventListener("click", close);

  const saveBtn = document.createElement("button");
  saveBtn.className = "primary";
  saveBtn.textContent = "保存";
  saveBtn.addEventListener("click", async () => {
    const updated: GraphMetadata = {
      name: nameEl.value.trim() || null,
      version: versionEl.value.trim() || null,
      description: descEl.value.trim() || null,
    };
    try {
      await graphs.updateMetadata(sessionId, updated);
      onSaved();
      close();
    } catch (e) {
      showErrorDialog("保存节点图信息失败: " + (e as Error).message);
    }
  });

  btns.appendChild(cancelBtn);
  btns.appendChild(saveBtn);
  dialog.appendChild(btns);

  setTimeout(() => nameEl.focus(), 0);
}

export async function openHyperparametersDialog(
  sessionId: string,
  onSaved: () => void,
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

  let connections: ConnectionConfig[] = [];
  try {
    connections = await system.connections.list();
  } catch (e) {
    alert("加载连接配置失败: " + (e as Error).message);
    close();
    return;
  }

  const connectionsByKind = new Map<string, ConnectionConfig[]>();
  for (const connection of connections) {
    const kind = String(connection.kind.type ?? "");
    const items = connectionsByKind.get(kind) ?? [];
    items.push(connection);
    connectionsByKind.set(kind, items);
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
    const isPasswordInit = hp?.data_type === "Password";
    if (isPasswordInit) valueEl.type = "password";

    const peekBtn = document.createElement("button");
    peekBtn.textContent = "👁";
    peekBtn.title = "按住查看";
    peekBtn.style.cssText = "padding:2px 5px;font-size:12px;flex-shrink:0;";
    peekBtn.hidden = !isPasswordInit;
    const showValue = () => {
      if (valueEl.type === "password") valueEl.type = "text";
    };
    const hideValue = () => {
      if (typeEl.value === "Password") valueEl.type = "password";
    };
    peekBtn.addEventListener("mousedown", showValue);
    peekBtn.addEventListener("mouseup", hideValue);
    peekBtn.addEventListener("mouseleave", hideValue);

    const valueWrap = document.createElement("div");
    valueWrap.style.cssText = "display:flex;gap:2px;align-items:center;";
    valueWrap.appendChild(valueEl);
    valueWrap.appendChild(peekBtn);

    typeEl.addEventListener("change", () => {
      const isPwd = typeEl.value === "Password";
      valueEl.type = isPwd ? "password" : "text";
      peekBtn.hidden = !isPwd;
    });

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
    row.appendChild(valueWrap);
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
        case "Integer":
          valuesMap[name] = parseInt(raw, 10) || 0;
          break;
        case "Float":
          valuesMap[name] = parseFloat(raw) || 0.0;
          break;
        case "Boolean":
          valuesMap[name] = raw === "true" || raw === "1" || raw === "yes";
          break;
        default:
          valuesMap[name] = raw;
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

export async function openVariablesDialog(
  sessionId: string,
  onSaved: () => void,
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
          try {
            initVal = JSON.parse(raw);
          } catch {
            initVal = raw;
          }
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
