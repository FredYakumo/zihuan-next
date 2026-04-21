import type { FunctionPortDef } from "./types";
import { dataTypeSelect } from "./data_types";

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

export function buildPortListEditor(
  container: HTMLElement,
  ports: FunctionPortDef[],
  _showDesc?: boolean,
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
      const i = items.findIndex((it) => it.nameEl === nameEl);
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
    .map((it) => ({ name: it.nameEl.value.trim(), data_type: it.typeEl.value }))
    .filter((p) => p.name);
}

export { isValidConnectionType } from "./data_types";
