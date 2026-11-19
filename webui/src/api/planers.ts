import { request, type ServiceWithRuntime } from "./client";
export interface PlanerMember { role_id: string; description: string; workspace_path: string | null }
export interface PlanerConfig {
  config_id: string; name: string; enabled: boolean; auto_start: boolean; avatar_url: string | null;
  llm_ref_id: string; prompt: string; updated_at: string; members: PlanerMember[];
  runtime?: { status: "running" | "stopped" };
}
export const planers = {
  list: () => request<PlanerConfig[]>("GET", "/system/planers"),
  get: (id: string) => request<PlanerConfig>("GET", `/system/planers/${encodeURIComponent(id)}`),
  save: (config: PlanerConfig) => request<PlanerConfig>(config.config_id ? "PUT" : "POST", `/system/planers${config.config_id ? "/" + encodeURIComponent(config.config_id) : ""}`, config),
  remove: (id: string) => request("DELETE", `/system/planers/${encodeURIComponent(id)}`),
  start: (id: string) => request("POST", `/system/planers/${encodeURIComponent(id)}/start`),
  stop: (id: string) => request("POST", `/system/planers/${encodeURIComponent(id)}/stop`),
};
// Presentation adapter only: Planers remain independent entities in configuration/runtime.
export function planerChatTarget(config: PlanerConfig): ServiceWithRuntime {
  return { config_id: config.config_id, name: config.name, enabled: config.enabled, auto_start: config.auto_start,
    is_default: false, updated_at: config.updated_at, avatar_url: config.avatar_url, tools: [],
    role_service_type: { type: "planer", llm_ref_id: config.llm_ref_id, member_count: config.members.length },
    runtime: { agent_id: config.config_id, instance_id: null, status: config.runtime?.status ?? "stopped", started_at: null, last_error: null } };
}
