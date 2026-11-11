// Hook relay for WeChatPadPro.
//
// The bridge POSTs every message to this process (same Docker network), which keeps a small
// in-memory queue. zihuan_service long-polls `GET /events` from the host through the published
// port, so nothing ever has to reach the host from inside the container — no
// `host.docker.internal`, no extra firewall rule.
//
//   GET /events?after=<id>&wait=<ms>  -> { cursor, events: [{ id, payload }] }
//                                        after=-1 starts at the current head
//   GET /health                       -> ok
//
// Environment: PORT, HOOK_SELF_URL, PAD_URLS, PAD_TOKEN, QUEUE_LIMIT.

import http from "node:http";

const PORT = Number(process.env.PORT ?? 8080);
const QUEUE_LIMIT = Number(process.env.QUEUE_LIMIT ?? 500);
const SELF_URL = process.env.HOOK_SELF_URL ?? `http://${process.env.HOSTNAME ?? "hook"}:${PORT}/hook`;
const PAD_TOKEN = process.env.PAD_TOKEN ?? "";
const PAD_URLS = (process.env.PAD_URLS ?? "http://wechatpadpro:8080,http://wechatpadpro:1238")
  .split(",")
  .map((value) => value.trim().replace(/\/$/, ""))
  .filter(Boolean);

let head = 0;
const events = [];
const waiters = new Set();

function push(payload) {
  const item = { id: ++head, payload };
  events.push(item);
  while (events.length > QUEUE_LIMIT) events.shift();
  for (const wake of [...waiters]) wake();
  waiters.clear();
}

function pending(after) {
  return events.filter((item) => item.id > after);
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on("data", (chunk) => {
      size += chunk.length;
      if (size > 4 * 1024 * 1024) {
        reject(new Error("payload too large"));
        req.destroy();
        return;
      }
      chunks.push(chunk);
    });
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
}

function reply(res, status, body) {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

async function handleEvents(url, res) {
  const requested = Number(url.searchParams.get("after") ?? "-1");
  // A negative cursor means "start now": replaying what queued while zihuan was down would
  // answer stale messages out of context.
  let after = Number.isFinite(requested) && requested >= 0 ? requested : head;
  const wait = Math.max(0, Math.min(Number(url.searchParams.get("wait") ?? "0") || 0, 30000));

  let batch = pending(after);
  if (batch.length === 0 && wait > 0) {
    await new Promise((resolve) => {
      const timer = setTimeout(() => {
        waiters.delete(done);
        resolve();
      }, wait);
      const done = () => {
        clearTimeout(timer);
        waiters.delete(done);
        resolve();
      };
      waiters.add(done);
    });
    batch = pending(after);
  }

  reply(res, 200, { cursor: head, events: batch });
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url ?? "/", "http://relay");
  if (req.method === "POST" && url.pathname.startsWith("/hook")) {
    readBody(req)
      .then((body) => {
        let payload;
        try {
          payload = JSON.parse(body);
        } catch {
          payload = { raw: body };
        }
        push(payload);
        reply(res, 200, { ok: true });
      })
      .catch((error) => reply(res, 400, { ok: false, error: String(error) }));
    return;
  }
  if (req.method === "GET" && url.pathname === "/events") {
    handleEvents(url, res).catch((error) => reply(res, 500, { ok: false, error: String(error) }));
    return;
  }
  if (url.pathname === "/health") {
    res.writeHead(200, { "content-type": "text/plain" });
    res.end("ok");
    return;
  }
  reply(res, 404, { ok: false, error: "not found" });
});

// The bridge keeps its webhook config in its own database, so re-registering on every start is
// both cheap and enough to recover from a bridge that was not ready yet. Builds differ in the
// registration endpoint and body shape, so both candidates are tried until one is accepted.
// The 861 build can answer HTTP 200 with a JSON `Code` other than 200 when the key is not
// bound to a logged-in account yet — that still counts as "not registered" and keeps retrying,
// so registration heals on its own once a valid account key is configured.
const WEBHOOK_CONFIG_CANDIDATES = [
  {
    path: "/webhook/Config",
    body: () => ({
      URL: SELF_URL,
      MessageTypes: ["1", "3", "49"],
      Enabled: true,
      IncludeSelfMessage: false,
    }),
  },
  {
    path: "/v1/webhook/Config",
    body: () => ({ webhookUrl: SELF_URL, types: [1, 3, 49], includeSelf: false }),
  },
];

async function registerWebhook() {
  for (;;) {
    for (const pad of PAD_URLS) {
      for (const candidate of WEBHOOK_CONFIG_CANDIDATES) {
        const target = `${pad}${candidate.path}`;
        try {
          const url = new URL(target);
          if (PAD_TOKEN) url.searchParams.set("key", PAD_TOKEN);
          const response = await fetch(url, {
            method: "POST",
            headers: {
              "content-type": "application/json",
              ...(PAD_TOKEN ? { authorization: `Bearer ${PAD_TOKEN}` } : {}),
            },
            body: JSON.stringify(candidate.body()),
          });
          if (!response.ok) {
            console.log(`[hook] ${target} answered HTTP ${response.status}`);
            continue;
          }
          const result = await response.json().catch(() => null);
          const code = Number.isInteger(result?.Code) ? result.Code : 200;
          if (code === 200 || code === 0) {
            console.log(`[hook] webhook registered: ${target} -> ${SELF_URL}`);
            return;
          }
          console.log(`[hook] ${target} Code ${code}: ${result?.Text ?? ""}`);
        } catch (error) {
          console.log(`[hook] ${target} unreachable: ${error?.message ?? error}`);
        }
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 5000));
  }
}

server.listen(PORT, () => {
  console.log(`[hook] listening on :${PORT}, callback ${SELF_URL}`);
  registerWebhook();
});
