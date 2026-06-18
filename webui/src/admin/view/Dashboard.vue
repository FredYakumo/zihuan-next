<template>
  <section class="page dashboard-page">
    <div class="page-hero">
      <h2>运行总览</h2>
      <div class="dashboard-stats">
        <div class="stat-item">
          <span class="muted">连接配置</span>
          <strong>{{ stats.connections }}</strong>
        </div>
        <div class="stat-divider"></div>
        <div class="stat-item">
          <span class="muted">模型配置</span>
          <strong>{{ stats.llm }}</strong>
        </div>
        <div class="stat-divider"></div>
        <div class="stat-item">
          <span class="muted">Service 数量</span>
          <strong>{{ stats.agents }}</strong>
        </div>
      </div>
    </div>

    <section v-if="servicesLoading && services.length === 0" class="panel">
      <div class="dashboard-loading-state" aria-live="polite">
        <span class="dashboard-loading-spinner"></span>
        <span>Service 加载中...</span>
      </div>
    </section>

    <section v-else-if="services.length > 0" class="panel">
      <div class="connection-grid dashboard-service-grid">
        <article
          v-for="service in services"
          :key="service.config_id"
          class="connection-card dashboard-service-card"
        >
          <div class="connection-card-header connection-card-header--stacked">
            <div class="connection-card-header-top">
              <div class="connection-card-badges">
                <span class="badge">{{ service.agent_type.type }}</span>
                <span class="badge" :class="service.enabled ? 'success' : ''">
                  {{ service.enabled ? "已启用" : "已停用" }}
                </span>
                <span class="badge" :class="statusTone(service.runtime.status)">
                  {{ runtimeBadgeText(service) }}
                </span>
                <span v-if="service.is_default" class="badge">default</span>
              </div>
            </div>
            <div class="dashboard-service-title">
              <img
                v-if="agentAvatarUrl(service)"
                :src="agentAvatarUrl(service)"
                alt="service avatar"
                class="dashboard-service-avatar"
              />
              <div v-else class="dashboard-service-avatar dashboard-service-avatar--fallback">
                {{ agentInitial(service.name) }}
              </div>
              <h4>{{ service.name }}</h4>
            </div>
          </div>

          <div class="connection-card-body">
            <div class="key-value">
              <strong>Config ID</strong>
              <span class="mono">{{ compactId(service.config_id) }}</span>
            </div>
            <div class="key-value">
              <strong>模型</strong>
              <span>{{ llmName(service) }}</span>
            </div>
            <div v-if="service.agent_type.type === 'http_stream'" class="key-value">
              <strong>Bind</strong>
              <span class="mono">{{ (service.agent_type as Record<string, unknown>).bind || '127.0.0.1:18080' }}</span>
            </div>
            <div v-else-if="service.agent_type.type === 'qq_chat'" class="key-value">
              <strong>Bot QQ</strong>
              <span class="mono">{{ service.qq_chat_profile?.bot_user_id || '未知' }}</span>
            </div>
            <div v-else class="key-value">
              <strong>工作模式</strong>
              <span>Dashboard Session Workspace</span>
            </div>
            <div v-if="service.runtime.last_error" class="key-value">
              <strong>最近错误</strong>
              <span>{{ service.runtime.last_error }}</span>
            </div>
          </div>

          <div class="connection-card-footer dashboard-service-footer">
            <button
              v-if="CHAT_ELIGIBLE_SERVICE_TYPES.has(service.agent_type.type)"
              class="btn primary dashboard-service-btn"
              :disabled="service.runtime.status !== 'running' || operatingId === service.config_id"
              @click="openChatModal(service.config_id)"
            >
              对话
            </button>
            <button
              class="btn dashboard-service-btn"
              :disabled="service.runtime.status === 'running' || operatingId === service.config_id"
              @click="startService(service.config_id)"
            >
              {{ operatingId === service.config_id && pendingAction === 'start' ? "启动中..." : "启动" }}
            </button>
            <button
              class="btn warn dashboard-service-btn"
              :disabled="service.runtime.status !== 'running' || operatingId === service.config_id"
              @click="stopService(service.config_id)"
            >
              {{ operatingId === service.config_id && pendingAction === 'stop' ? "停止中..." : "停止" }}
            </button>
          </div>
        </article>
      </div>
    </section>

    <section v-else class="panel">
      <div class="empty-state">当前没有 Service。</div>
    </section>

    <Teleport to="body">
      <div
        v-if="chatModalAgentId"
        class="chat-modal-backdrop"
        @click.self="closeChatModal"
      >
        <div class="chat-modal-dialog">
          <div class="chat-modal-header">
            <div class="chat-modal-title">
              <img
                v-if="chatModalService && agentAvatarUrl(chatModalService)"
                :src="agentAvatarUrl(chatModalService)"
                alt="service avatar"
                class="chat-modal-avatar"
              />
              <div
                v-else-if="chatModalService"
                class="chat-modal-avatar chat-modal-avatar--fallback"
              >
                {{ agentInitial(chatModalService.name) }}
              </div>
              <h3>{{ chatModalService?.name || "Chat" }}</h3>
            </div>
            <div class="chat-modal-actions">
              <button class="btn ghost" @click="openChatInNewWindow">在新窗口打开</button>
              <button class="chat-modal-close" @click="closeChatModal">✕</button>
            </div>
          </div>
          <div class="chat-modal-body">
            <Chat
              :agent-id="chatModalAgentId"
              :session-id="chatModalSessionId"
              embedded
              @update:session-id="chatModalSessionId = $event"
            />
          </div>
        </div>
      </div>
    </Teleport>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";

import { system, type ServiceWithRuntime, type LlmConfig } from "../../api/client";
import {
  statusTone,
  compactId,
  agentAvatarUrl,
  agentInitial,
  CHAT_ELIGIBLE_SERVICE_TYPES,
} from "../model";
import Chat from "./Chat.vue";

const router = useRouter();

const services = ref<ServiceWithRuntime[]>([]);
const servicesLoading = ref(false);
const llmModels = ref<LlmConfig[]>([]);
const operatingId = ref("");
const pendingAction = ref<"start" | "stop" | "">("");
const chatModalAgentId = ref("");
const chatModalSessionId = ref("");

const stats = reactive({
  connections: 0,
  llm: 0,
  agents: 0,
});

const chatModalService = computed(() =>
  services.value.find((service) => service.config_id === chatModalAgentId.value),
);

function llmName(service: ServiceWithRuntime): string {
  const agentType = service.agent_type as Record<string, unknown>;
  const llmId = String(agentType.llm_ref_id ?? "");
  if (!llmId) {
    return "未绑定";
  }
  return llmModels.value.find((item) => item.config_id === llmId)?.name ?? "未知";
}

function runtimeBadgeText(service: ServiceWithRuntime): string {
  switch (service.runtime.status) {
    case "running":
      return service.runtime.instance_id
        ? `已启动 (${compactId(service.runtime.instance_id)})`
        : "已启动";
    case "stopped":
      return "已停止";
    case "starting":
      return "启动中";
    case "error":
      return "启动失败";
    default:
      return service.runtime.status;
  }
}

async function startService(id: string) {
  if (operatingId.value) {
    return;
  }
  operatingId.value = id;
  pendingAction.value = "start";
  try {
    await system.services.start(id);
    await load();
  } catch (error) {
    alert(`启动失败: ${(error as Error).message}`);
  } finally {
    operatingId.value = "";
    pendingAction.value = "";
  }
}

async function stopService(id: string) {
  if (operatingId.value) {
    return;
  }
  operatingId.value = id;
  pendingAction.value = "stop";
  try {
    await system.services.stop(id);
    await load();
  } catch (error) {
    alert(`停止失败: ${(error as Error).message}`);
  } finally {
    operatingId.value = "";
    pendingAction.value = "";
  }
}

function openChatModal(agentId: string) {
  chatModalAgentId.value = agentId;
  chatModalSessionId.value = "";
}

function closeChatModal() {
  chatModalAgentId.value = "";
  chatModalSessionId.value = "";
}

function openChatInNewWindow() {
  if (!chatModalAgentId.value) {
    return;
  }
  const query: Record<string, string> = { agent_id: chatModalAgentId.value };
  if (chatModalSessionId.value) {
    query.session_id = chatModalSessionId.value;
  }
  const routeUrl = router.resolve({ path: "/chat", query });
  window.open(routeUrl.href, "_blank");
}

async function load() {
  servicesLoading.value = true;
  try {
    const [connections, llm, loadedAgents] = await Promise.all([
      system.connections.list(),
      system.llm.list(),
      system.services.list(),
    ]);
    stats.connections = connections.length;
    stats.llm = llm.length;
    stats.agents = loadedAgents.length;
    services.value = loadedAgents;
    llmModels.value = llm;
  } finally {
    servicesLoading.value = false;
  }
}

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`仪表盘加载失败: ${(error as Error).message}`);
  });
});
</script>

<style scoped lang="scss">
.dashboard-page {
  gap: 16px;
  display: flex;
  flex-direction: column;
}

.dashboard-stats {
  display: flex;
  align-items: center;
  gap: 24px;
}

.stat-item {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
}

.stat-item .muted {
  font-size: 13px;
  margin-bottom: 4px;
}

.stat-item strong {
  font-size: 24px;
  line-height: 1;
  color: var(--admin-accent);
}

.stat-divider {
  width: 1px;
  height: 36px;
  background: var(--admin-border);
}

.dashboard-loading-state {
  min-height: 180px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--admin-subtle);
}

.dashboard-loading-spinner {
  width: 18px;
  height: 18px;
  border: 2px solid color-mix(in srgb, var(--admin-accent) 28%, transparent);
  border-top-color: var(--admin-accent);
  border-radius: 50%;
  animation: dashboard-spin 0.75s linear infinite;
  flex-shrink: 0;
}

@keyframes dashboard-spin {
  to {
    transform: rotate(360deg);
  }
}

.dashboard-service-card {
  display: flex;
  flex-direction: column;
  min-height: auto;
  padding: 10px;
  gap: 6px;
  border-radius: 16px;
}

.dashboard-service-title {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 2px;
}

.dashboard-service-title h4 {
  margin: 0;
  font-size: 16px;
}

.dashboard-service-avatar {
  width: 28px;
  height: 28px;
  border-radius: 999px;
  object-fit: cover;
  border: 1px solid var(--admin-border);
  background: var(--admin-bg-soft);
}

.dashboard-service-avatar--fallback {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--admin-accent) 18%, var(--admin-bg-panel) 82%);
  color: var(--admin-ink);
  font-size: 12px;
  font-weight: 700;
}

.dashboard-service-footer {
  display: flex;
  gap: 6px;
  justify-content: flex-end;
  margin-top: 6px;
}

.dashboard-service-btn {
  flex: 1;
  min-width: 0;
  padding: 6px 10px;
  font-size: 13px;
}

.dashboard-service-card .connection-card-body {
  gap: 4px;
}

.dashboard-service-card .key-value {
  font-size: 12px;
}

.dashboard-service-card .key-value strong {
  min-width: 52px;
}

.dashboard-service-card .connection-card-badges {
  gap: 5px;
}

.dashboard-service-card .connection-card-badges .badge {
  font-size: 11px;
  padding: 3px 6px;
  border-radius: 4px;
}

.dashboard-service-card .connection-card-header-top {
  gap: 4px;
}

.dashboard-service-grid {
  grid-template-columns: repeat(auto-fit, 260px);
  gap: 12px;
}

.chat-modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: grid;
  place-items: center;
  padding: 16px;
  overflow: hidden;
  background: color-mix(in srgb, var(--bg) 55%, transparent 45%);
  backdrop-filter: blur(12px);
}

.chat-modal-dialog {
  width: 90vw;
  height: 85vh;
  display: flex;
  flex-direction: column;
  padding: 0;
  border-radius: 20px;
  border: 1px solid var(--admin-border);
  background: linear-gradient(
    180deg,
    color-mix(in srgb, var(--admin-bg-panel) 94%, transparent 6%),
    color-mix(in srgb, var(--admin-bg-panel-strong) 98%, transparent 2%)
  );
  box-shadow: var(--admin-card-shadow);
  overflow: hidden;
}

.chat-modal-header {
  flex-shrink: 0;
  padding: 14px 20px;
  border-bottom: 1px solid var(--admin-border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.chat-modal-title {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.chat-modal-title h3 {
  margin: 0;
  font-size: 16px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.chat-modal-avatar {
  width: 32px;
  height: 32px;
  border-radius: 999px;
  object-fit: cover;
  border: 1px solid var(--admin-border);
  flex-shrink: 0;
}

.chat-modal-avatar--fallback {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--admin-accent) 18%, var(--admin-bg-panel) 82%);
  color: var(--admin-ink);
  font-size: 12px;
  font-weight: 700;
  flex-shrink: 0;
}

.chat-modal-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}

.chat-modal-close {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--admin-subtle);
  font-size: 18px;
  line-height: 1;
  cursor: pointer;
  display: grid;
  place-items: center;
  transition: all 0.15s;
}

.chat-modal-close:hover {
  background: color-mix(in srgb, var(--admin-danger, #ef4444) 12%, transparent);
  color: var(--admin-danger, #ef4444);
}

.chat-modal-body {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

@media (max-width: 900px) {
  .dashboard-stats {
    flex-wrap: wrap;
    gap: 14px;
  }

  .stat-divider {
    display: none;
  }

  .chat-modal-dialog {
    width: 100vw;
    height: 100vh;
    border-radius: 0;
  }

  .chat-modal-header {
    padding: 12px 16px;
  }

  .dashboard-service-footer {
    flex-wrap: wrap;
  }
}
</style>
