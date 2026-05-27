import type { FunctionPortDef } from "./types";
import { cloneDataTypeMetaData, dataTypeSelect, parseDisplayDataType } from "./data_types";

export function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\"/g, "&quot;");
}

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

type PortRowItem = {
  nameEl: HTMLInputElement;
  typeEl: HTMLSelectElement;
  descEl?: HTMLInputElement;
  row: HTMLElement;
  reserved: boolean;
};

export function buildPortListEditor(
  container: HTMLElement,
  ports: FunctionPortDef[],
  showDesc = false,
  excludeDataTypes?: string[],
  reservedPorts?: FunctionPortDef[],
): () => FunctionPortDef[] {
  const items: PortRowItem[] = [];

  function clearRowError(row: HTMLElement) {
    row.style.border = "";
    row.style.borderRadius = "";
    const existing = row.querySelector(".port-duplicate-error");
    if (existing) existing.remove();
  }

  function setRowError(row: HTMLElement, msg: string) {
    row.style.border = "1px solid #e74c3c";
    row.style.borderRadius = "4px";
    let err = row.querySelector(".port-duplicate-error") as HTMLElement | null;
    if (!err) {
      err = document.createElement("span");
      err.className = "port-duplicate-error";
      err.style.cssText = "color:#e74c3c;font-size:11px;white-space:nowrap;align-self:center;";
      row.appendChild(err);
    }
    err.textContent = msg;
  }

  function validateDuplicates() {
    const nameToRows = new Map<string, HTMLElement[]>();
    for (const it of items) {
      const name = it.nameEl.value.trim();
      if (name) {
        const list = nameToRows.get(name) ?? [];
        list.push(it.row);
        nameToRows.set(name, list);
      }
    }
    for (const it of items) {
      clearRowError(it.row);
    }
    for (const [name, rows] of nameToRows) {
      if (rows.length > 1) {
        for (const row of rows) {
          setRowError(row, `重复名称: ${name}`);
        }
      }
    }
  }

  const addBtn = document.createElement("button");
  addBtn.textContent = "+ 添加端口";
  addBtn.style.marginBottom = "8px";

  const addRow = (port?: FunctionPortDef, reserved = false) => {
    const row = document.createElement("div");
    row.className = "zh-port-row";

    const nameEl = document.createElement("input");
    nameEl.type = "text";
    nameEl.placeholder = "port_name";
    nameEl.value = port?.name ?? "";
    nameEl.style.marginBottom = "0";

    const typeEl = dataTypeSelect(port?.data_type ?? "String", undefined, excludeDataTypes);
    typeEl.style.marginBottom = "0";

    const descEl = showDesc ? document.createElement("input") : undefined;
    if (descEl) {
      descEl.type = "text";
      descEl.placeholder = "description";
      descEl.value = port?.description ?? "";
      descEl.style.marginBottom = "0";
    }

    if (reserved) {
      nameEl.readOnly = true;
      typeEl.disabled = true;
      if (descEl) descEl.readOnly = true;

      row.appendChild(nameEl);
      row.appendChild(typeEl);
      if (descEl) row.appendChild(descEl);
      container.insertBefore(row, addBtn);
      items.push({ nameEl, typeEl, descEl, row, reserved: true });
    } else {
      const removeBtn = document.createElement("button");
      removeBtn.textContent = "✕";
      removeBtn.className = "danger";
      removeBtn.style.padding = "4px 8px";
      removeBtn.addEventListener("click", () => {
        const i = items.findIndex((it) => it.nameEl === nameEl);
        if (i >= 0) items.splice(i, 1);
        row.remove();
        validateDuplicates();
      });

      nameEl.addEventListener("input", () => validateDuplicates());

      row.appendChild(nameEl);
      row.appendChild(typeEl);
      if (descEl) row.appendChild(descEl);
      row.appendChild(removeBtn);
      container.insertBefore(row, addBtn);
      items.push({ nameEl, typeEl, descEl, row, reserved: false });
    }
  };

  addBtn.addEventListener("click", () => {
    addRow();
    validateDuplicates();
  });
  container.appendChild(addBtn);

  for (const p of (reservedPorts ?? [])) addRow(p, true);
  for (const p of ports) addRow(p, false);

  validateDuplicates();

  return () =>
    items
      .filter((it) => !it.reserved)
      .map((it) => ({
        name: it.nameEl.value.trim(),
        data_type: cloneDataTypeMetaData(parseDisplayDataType(it.typeEl.value)),
        description: it.descEl?.value.trim() ?? "",
      }))
      .filter((p) => p.name);
}

export { isValidConnectionType } from "./data_types";
