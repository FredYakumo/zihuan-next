import type { ServerMessage } from "../api/types";
import type { ZihuanWS } from "../api/ws";

export interface TaskRuntimeBindings {
  getActiveTabId: () => string | null;
  setCurrentTaskId: (taskId: string | null) => void;
  setRunningSessionId: (sessionId: string | null) => void;
  updateRunButton: (isRunning: boolean) => void;
  addLog: (level: string, message: string) => void;
  appendLogEntry: (level: string, message: string, timestamp: string) => void;
}

export function registerTaskRuntimeHandlers(
  socket: ZihuanWS,
  bindings: TaskRuntimeBindings,
): () => void {
  return socket.onMessage((msg: ServerMessage) => {
    if (msg.type === "TaskStarted") {
      bindings.setCurrentTaskId(msg.task_id);
      bindings.setRunningSessionId(msg.graph_session_id);
      if (msg.graph_session_id === bindings.getActiveTabId()) {
        bindings.updateRunButton(true);
      }
      return;
    }

    if (msg.type === "TaskFinished" || msg.type === "TaskStopped") {
      bindings.setCurrentTaskId(null);
      bindings.setRunningSessionId(null);
      bindings.updateRunButton(false);
      return;
    }

    if (msg.type === "LogMessage") {
      bindings.addLog(msg.level, msg.message);
      bindings.appendLogEntry(msg.level, msg.message, msg.timestamp);
    }
  });
}
