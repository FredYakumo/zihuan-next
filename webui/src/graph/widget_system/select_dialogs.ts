import type {
  ConnectionConfig,
  LlmConfig,
} from "../../api/client";
import { ensureDialogStyles, openOverlay } from "../../ui/dialogs/base";
import {
  AGENT_LLM_KIND_OPTIONS,
  CONNECTION_PLACEHOLDER_VALUE,
  LLM_PLACEHOLDER_VALUE,
  type ActiveBotAdapterConnection,
  buildConnectionValueMap,
  buildLlmRefValueMap,
  matchesConnectionKind,
} from "./select_options";

export async function showActiveBotAdapterPicker(
  connections: ActiveBotAdapterConnection[],
  selectedId: string,
): Promise<string | null> {
  ensureDialogStyles();
  const { overlay, dialog, close } = openOverlay();
  dialog.style.minWidth = "360px";
  dialog.style.maxWidth = "420px";

  const title = document.createElement("h3");
  title.textContent = "选择 IMS Bot Adapter";
  dialog.appendChild(title);

  const list = document.createElement("div");
  list.style.cssText = "display:flex;flex-direction:column;gap:8px;margin-top:12px;";

  const allItems = [
    { connection_id: CONNECTION_PLACEHOLDER_VALUE, name: "不选择", ws_url: "" },
    ...connections,
  ];

  if (allItems.length === 1) {
    const empty = document.createElement("div");
    empty.className = "zh-hint";
    empty.textContent = "当前没有已激活的 Bot Adapter。";
    dialog.appendChild(empty);
  } else {
    for (const item of allItems) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = item.connection_id === CONNECTION_PLACEHOLDER_VALUE
        ? item.name
        : `${item.name} (${item.ws_url})`;
      button.style.cssText = "text-align:left;";
      if (item.connection_id === selectedId) {
        button.className = "primary";
      }
      button.addEventListener("click", () => {
        close();
        resolvePromise(item.connection_id);
      });
      list.appendChild(button);
    }
    dialog.appendChild(list);
  }

  const footer = document.createElement("div");
  footer.className = "zh-buttons";
  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  footer.appendChild(cancelBtn);
  dialog.appendChild(footer);

  let resolved = false;
  const resolvePromise = (value: string | null) => {
    if (resolved) return;
    resolved = true;
    resolver(value);
  };
  cancelBtn.addEventListener("click", () => {
    close();
    resolvePromise(null);
  });
  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) {
      close();
      resolvePromise(null);
    }
  });

  let resolver: (value: string | null) => void = () => {};
  return new Promise<string | null>((resolve) => {
    resolver = resolve;
  });
}

export async function showConnectionPicker(
  connections: ConnectionConfig[],
  connectionKind: string,
  selectedId: string,
): Promise<string | null> {
  ensureDialogStyles();
  const { overlay, dialog, close } = openOverlay();
  dialog.style.minWidth = "360px";
  dialog.style.maxWidth = "460px";

  const title = document.createElement("h3");
  title.textContent = "选择连接配置";
  dialog.appendChild(title);

  const list = document.createElement("div");
  list.style.cssText = "display:flex;flex-direction:column;gap:8px;margin-top:12px;";

  const filteredConnections = connections.filter((item) => (
    item.enabled && matchesConnectionKind(String(item.kind.type ?? ""), connectionKind)
  ));
  const valueMap = buildConnectionValueMap(filteredConnections, connectionKind, selectedId);
  const allItems = Object.entries(valueMap);

  if (allItems.length === 1) {
    const empty = document.createElement("div");
    empty.className = "zh-hint";
    empty.textContent = "当前没有可用的连接配置。";
    dialog.appendChild(empty);
  } else {
    for (const [connectionId, label] of allItems) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = label;
      button.style.cssText = "text-align:left;";
      if (connectionId === selectedId) {
        button.className = "primary";
      }
      button.addEventListener("click", () => {
        close();
        resolvePromise(connectionId);
      });
      list.appendChild(button);
    }
    dialog.appendChild(list);
  }

  const footer = document.createElement("div");
  footer.className = "zh-buttons";
  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  footer.appendChild(cancelBtn);
  dialog.appendChild(footer);

  let resolved = false;
  const resolvePromise = (value: string | null) => {
    if (resolved) return;
    resolved = true;
    resolver(value);
  };
  cancelBtn.addEventListener("click", () => {
    close();
    resolvePromise(null);
  });
  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) {
      close();
      resolvePromise(null);
    }
  });

  let resolver: (value: string | null) => void = () => {};
  return new Promise<string | null>((resolve) => {
    resolver = resolve;
  });
}

export async function showAgentLlmKindPicker(
  selectedKind: string,
): Promise<string | null> {
  ensureDialogStyles();
  const { overlay, dialog, close } = openOverlay();
  dialog.style.minWidth = "360px";
  dialog.style.maxWidth = "420px";

  const title = document.createElement("h3");
  title.textContent = "选择 Agent LLM 类型";
  dialog.appendChild(title);

  const list = document.createElement("div");
  list.style.cssText = "display:flex;flex-direction:column;gap:8px;margin-top:12px;";
  for (const item of AGENT_LLM_KIND_OPTIONS) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = item.label;
    button.style.cssText = "text-align:left;";
    if (item.value === selectedKind) {
      button.className = "primary";
    }
    button.addEventListener("click", () => {
      close();
      resolvePromise(item.value);
    });
    list.appendChild(button);
  }
  dialog.appendChild(list);

  const footer = document.createElement("div");
  footer.className = "zh-buttons";
  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  footer.appendChild(cancelBtn);
  dialog.appendChild(footer);

  let resolved = false;
  const resolvePromise = (value: string | null) => {
    if (resolved) return;
    resolved = true;
    resolver(value);
  };
  cancelBtn.addEventListener("click", () => {
    close();
    resolvePromise(null);
  });
  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) {
      close();
      resolvePromise(null);
    }
  });

  let resolver: (value: string | null) => void = () => {};
  return new Promise<string | null>((resolve) => {
    resolver = resolve;
  });
}

export async function showLlmRefPicker(
  llmRefs: LlmConfig[],
  selectedId: string,
): Promise<string | null> {
  ensureDialogStyles();
  const { overlay, dialog, close } = openOverlay();
  dialog.style.minWidth = "360px";
  dialog.style.maxWidth = "460px";

  const title = document.createElement("h3");
  title.textContent = "选择 LLM 配置";
  dialog.appendChild(title);

  const list = document.createElement("div");
  list.style.cssText = "display:flex;flex-direction:column;gap:8px;margin-top:12px;";

  const valueMap = buildLlmRefValueMap(llmRefs, selectedId);
  const allItems = Object.entries(valueMap);

  if (allItems.length === 1) {
    const empty = document.createElement("div");
    empty.className = "zh-hint";
    empty.textContent = "当前没有可用的聊天 LLM 配置。";
    dialog.appendChild(empty);
  } else {
    for (const [llmRefId, label] of allItems) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = label;
      button.style.cssText = "text-align:left;";
      if (llmRefId === selectedId) {
        button.className = "primary";
      }
      button.addEventListener("click", () => {
        close();
        resolvePromise(llmRefId);
      });
      list.appendChild(button);
    }
    dialog.appendChild(list);
  }

  const footer = document.createElement("div");
  footer.className = "zh-buttons";
  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  footer.appendChild(cancelBtn);
  dialog.appendChild(footer);

  let resolved = false;
  const resolvePromise = (value: string | null) => {
    if (resolved) return;
    resolved = true;
    resolver(value);
  };
  cancelBtn.addEventListener("click", () => {
    close();
    resolvePromise(null);
  });
  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) {
      close();
      resolvePromise(null);
    }
  });

  let resolver: (value: string | null) => void = () => {};
  return new Promise<string | null>((resolve) => {
    resolver = resolve;
  });
}

export async function showTextEmbeddingModelPicker(
  models: string[],
  selectedModel: string,
): Promise<string | null> {
  ensureDialogStyles();
  const { overlay, dialog, close } = openOverlay();
  dialog.style.minWidth = "360px";
  dialog.style.maxWidth = "460px";

  const title = document.createElement("h3");
  title.textContent = "选择 Embedding 模型";
  dialog.appendChild(title);

  const list = document.createElement("div");
  list.style.cssText = "display:flex;flex-direction:column;gap:8px;margin-top:12px;";

  if (models.length === 0) {
    const empty = document.createElement("div");
    empty.className = "zh-hint";
    empty.textContent = "当前没有可用的本地 Embedding 模型。";
    dialog.appendChild(empty);
  } else {
    for (const model of models) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = model;
      button.style.cssText = "text-align:left;";
      if (model === selectedModel) {
        button.className = "primary";
      }
      button.addEventListener("click", () => {
        close();
        resolvePromise(model);
      });
      list.appendChild(button);
    }
    dialog.appendChild(list);
  }

  const footer = document.createElement("div");
  footer.className = "zh-buttons";
  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "取消";
  footer.appendChild(cancelBtn);
  dialog.appendChild(footer);

  let resolved = false;
  const resolvePromise = (value: string | null) => {
    if (resolved) return;
    resolved = true;
    resolver(value);
  };
  cancelBtn.addEventListener("click", () => {
    close();
    resolvePromise(null);
  });
  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) {
      close();
      resolvePromise(null);
    }
  });

  let resolver: (value: string | null) => void = () => {};
  return new Promise<string | null>((resolve) => {
    resolver = resolve;
  });
}
