// WebSocket client for real-time server events

import type { ServerMessage, ClientMessage } from "./types";

type MessageHandler = (msg: ServerMessage) => void;

export class ZihuanWS {
  private ws: WebSocket | null = null;
  private handlers: MessageHandler[] = [];
  private reconnectHandlers = new Set<() => void>();
  private hasConnected = false;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private shouldReconnect = true;

  connect(): void {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${proto}//${location.host}/api/ws`;
    this.ws = new WebSocket(url);

    this.ws.onopen = () => {
      console.log("[WS] Connected");
      if (this.hasConnected) this.reconnectHandlers.forEach((handler) => handler());
      this.hasConnected = true;
      if (this.reconnectTimer !== null) {
        clearTimeout(this.reconnectTimer);
        this.reconnectTimer = null;
      }
    };

    this.ws.onmessage = (event) => {
      let msg: ServerMessage;
      try {
        msg = JSON.parse(event.data as string) as ServerMessage;
      } catch {
        console.error("[WS] Invalid server message");
        return;
      }
      this.handlers.forEach((handler) => {
        try { handler(msg); }
        catch (error) { console.error("[WS] Event handler failed", error); }
      });
    };

    this.ws.onclose = () => {
      if (this.shouldReconnect) {
        this.reconnectTimer = setTimeout(() => this.connect(), 3000);
      }
    };

    this.ws.onerror = () => {
      this.ws?.close();
    };
  }

  disconnect(): void {
    this.shouldReconnect = false;
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
    }
    this.ws?.close();
  }

  send(msg: ClientMessage): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg));
    }
  }

  onReconnect(handler: () => void): () => void {
    this.reconnectHandlers.add(handler);
    return () => this.reconnectHandlers.delete(handler);
  }

  onMessage(handler: MessageHandler): () => void {
    this.handlers.push(handler);
    return () => {
      this.handlers = this.handlers.filter((h) => h !== handler);
    };
  }

  subscribe(graphId: string): void {
    this.send({ type: "Subscribe", graph_id: graphId });
  }

  unsubscribe(graphId: string): void {
    this.send({ type: "Unsubscribe", graph_id: graphId });
  }
}

export const ws = new ZihuanWS();
