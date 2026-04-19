// Frontend logger — forwards log messages to the Rust backend via POST /api/log,
// which emits them through LogUtil (console + file output, WebSocket broadcast).

const BASE = "/api";

type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

function send(level: LogLevel, message: string): void {
  fetch(`${BASE}/log`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ level, message }),
  }).catch(() => {
    // Fallback: never let the logger itself throw
    console.warn("[logger] failed to send log to backend:", message);
  });
}

export const logger = {
  error: (msg: string) => send("error", msg),
  warn:  (msg: string) => send("warn",  msg),
  info:  (msg: string) => send("info",  msg),
  debug: (msg: string) => send("debug", msg),
  trace: (msg: string) => send("trace", msg),
};
