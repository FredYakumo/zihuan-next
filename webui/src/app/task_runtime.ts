import type { ServerMessage } from "../api/types";
import type { ZihuanWS } from "../api/ws";

export interface TaskRuntimeBindings {
  onTaskLifecycleChanged: () => void;
  addLog: (level: string, message: string) => void;
  appendLogEntry: (level: string, message: string, timestamp: string) => void;
}

export function registerTaskRuntimeHandlers(
  socket: ZihuanWS,
  bindings: TaskRuntimeBindings,
): () => void {
  return socket.onMessage((msg: ServerMessage) => {
    if (msg.type === "TaskStarted" || msg.type === "TaskFinished" || msg.type === "TaskStopped") {
      bindings.onTaskLifecycleChanged();
      return;
    }

    if (msg.type === "LogMessage") {
      bindings.addLog(msg.level, msg.message);
      bindings.appendLogEntry(msg.level, msg.message, msg.timestamp);
    }
  });
}
