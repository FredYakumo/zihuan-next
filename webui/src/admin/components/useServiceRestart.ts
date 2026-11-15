import { reactive } from "vue";

import { system } from "../../api/client";
import { ws } from "../../api/ws";
import type { DependentRoleService } from "../../api/types";

export interface ServiceRestartLogLine {
  timestamp: string;
  level: string;
  message: string;
}

export type ServiceRestartPhase = "confirm" | "restarting" | "done" | "failed";
export type ServiceRestartMode = "save" | "deleted";

const MAX_LOG_LINES = 500;

export interface ServiceRestartDialogState {
  visible: boolean;
  mode: ServiceRestartMode;
  phase: ServiceRestartPhase;
  services: DependentRoleService[];
  logLines: ServiceRestartLogLine[];
  progressText: string;
  errorMessage: string;
}

/**
 * How the owning page settles its editor once the dialog resolves.
 *
 * The editor stays open while the dialog shows on top of it, so "取消" simply
 * dismisses the dialog and returns to the edit page. "仅保存" and a successful
 * restart both finish the flow by closing the editor through these callbacks.
 */
export interface ServiceRestartCallbacks {
  /** Config saved and affected services restarted; close the editor and refresh. */
  onCompleted?: () => void | Promise<void>;
  /** Config saved without restart ("仅保存"); close the editor and refresh. */
  onSaveOnly?: () => void | Promise<void>;
}

/**
 * Shared save-and-restart flow: a save/delete mutation returns the running role
 * services referencing the config (`affected_running_services`); when non-empty
 * the dialog asks whether to restart them, streaming the restart logs (WS
 * `ServiceRuntimeLog`) into the dialog.
 *
 * Delete flows open the dialog in `deleted` mode: the backend stops the
 * affected services itself (a deleted config cannot be restarted onto), so the
 * dialog is informational only.
 */
export function useServiceRestart(callbacks: ServiceRestartCallbacks = {}) {
  const state = reactive<ServiceRestartDialogState>({
    visible: false,
    mode: "save",
    phase: "confirm",
    services: [],
    logLines: [],
    progressText: "",
    errorMessage: "",
  });
  let unsubscribe: (() => void) | null = null;

  function reset(): void {
    state.phase = "confirm";
    state.logLines = [];
    state.progressText = "";
    state.errorMessage = "";
  }

  function open(affected: DependentRoleService[] | undefined | null, mode: ServiceRestartMode): boolean {
    if (!affected?.length) {
      return false;
    }
    state.mode = mode;
    state.services = affected;
    reset();
    state.visible = true;
    return true;
  }

  /** Opens the restart confirmation dialog; returns false when nothing is affected. */
  function openAfterSave(affected: DependentRoleService[] | undefined | null): boolean {
    return open(affected, "save");
  }

  /** Opens the informational dialog listing the services the backend stopped. */
  function openAfterDelete(affected: DependentRoleService[] | undefined | null): boolean {
    return open(affected, "deleted");
  }

  function pushLog(level: string, message: string, timestamp: string): void {
    state.logLines.push({ timestamp, level, message });
    if (state.logLines.length > MAX_LOG_LINES) {
      state.logLines.splice(0, state.logLines.length - MAX_LOG_LINES);
    }
  }

  function now(): string {
    return new Date().toLocaleString("zh-CN", { hour12: false });
  }

  async function restartAll(): Promise<void> {
    if (state.phase === "restarting") {
      return;
    }
    state.phase = "restarting";
    state.errorMessage = "";
    try {
      for (let index = 0; index < state.services.length; index += 1) {
        const service = state.services[index];
        state.progressText = `正在重启（${index + 1}/${state.services.length}）：${service.name}`;
        pushLog("INFO", `===== 重启 Role Service：${service.name}（${service.id}）=====`, now());
        const currentServiceId = service.id;
        unsubscribe?.();
        unsubscribe = ws.onMessage((msg) => {
          if (msg.type === "ServiceRuntimeLog" && msg.service_id === currentServiceId) {
            pushLog(msg.level, msg.message, msg.timestamp);
          }
        });
        try {
          await system.services.restart(service.id);
          pushLog("INFO", `服务 ${service.name} 重启完成`, now());
        } finally {
          unsubscribe?.();
          unsubscribe = null;
        }
      }
      state.progressText = "";
      state.phase = "done";
      await callbacks.onCompleted?.();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      state.errorMessage = message;
      pushLog("ERROR", `重启失败：${message}`, now());
      state.phase = "failed";
    }
  }

  /** "仅保存": config is already saved; finish the flow without restarting. */
  async function saveOnly(): Promise<void> {
    close();
    await callbacks.onSaveOnly?.();
  }

  /** "取消": dismiss the dialog; the page keeps its editor open. */
  function close(): void {
    unsubscribe?.();
    unsubscribe = null;
    state.visible = false;
  }

  return { state, openAfterSave, openAfterDelete, restartAll, saveOnly, close };
}

export type UseServiceRestartReturn = ReturnType<typeof useServiceRestart>;
