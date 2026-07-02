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

    <section
      v-if="notificationCards.length > 0"
      class="panel dashboard-privilege-panel"
    >
      <div class="dashboard-section-header">
        <div>
          <h3>通知</h3>
        </div>
        <button
          class="btn warn dashboard-clear-btn"
          :disabled="clearingNotifications"
          @click="clearAllNotifications"
        >
          {{ clearingNotifications ? "清空中..." : "清空" }}
        </button>
      </div>
      <div class="connection-grid dashboard-privilege-grid">
        <article
          v-for="card in notificationCards"
          :key="`${card.agent_id}-${card.id}`"
          class="connection-card dashboard-service-card dashboard-privilege-card"
        >
          <div class="connection-card-header connection-card-header--stacked">
            <div class="connection-card-header-top">
              <div class="connection-card-badges">
                <span class="badge">privilege</span>
                <span class="badge" :class="card.consumed ? '' : 'success'">
                  {{ card.consumed ? "已消费" : "待验证" }}
                </span>
                <span v-if="card.elevated_until" class="badge success">已提权</span>
              </div>
            </div>
            <div class="dashboard-service-title">
              <div class="dashboard-service-avatar dashboard-service-avatar--fallback">
                {{ card.agentName.slice(0, 1) }}
              </div>
              <h4>{{ card.agentName }}</h4>
            </div>
          </div>

          <div class="connection-card-body">
            <div class="key-value">
              <strong>用户</strong>
              <span class="mono">{{ card.sender_id }}</span>
            </div>
            <div class="key-value">
              <strong>用途</strong>
              <span>{{ card.purpose }}</span>
            </div>
            <div class="key-value">
              <strong>失败次数</strong>
              <span>{{ card.failed_attempts }}</span>
            </div>
            <div class="key-value">
              <strong>过期时间</strong>
              <span>{{ card.expires_at }}</span>
            </div>
            <div v-if="card.elevated_until" class="key-value">
              <strong>提权至</strong>
              <span>{{ card.elevated_until }}</span>
            </div>
          </div>

          <div class="connection-card-footer dashboard-service-footer">
            <button class="btn dashboard-service-btn" @click="openNotificationKeyModal(card)">
              查看密钥
            </button>
          </div>
        </article>
      </div>
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

    <Teleport to="body">
      <div
        v-if="selectedNotificationCard"
        class="chat-modal-backdrop"
        @click.self="selectedNotificationCard = null"
      >
        <div class="dashboard-secret-dialog">
          <div class="chat-modal-header">
            <div class="chat-modal-title">
              <div class="dashboard-service-avatar dashboard-service-avatar--fallback">
                {{ selectedNotificationCard.agentName.slice(0, 1) }}
              </div>
              <h3>{{ selectedNotificationCard.agentName }} 密钥</h3>
            </div>
            <div class="chat-modal-actions">
              <button class="chat-modal-close" @click="selectedNotificationCard = null">✕</button>
            </div>
          </div>
          <div class="dashboard-secret-body">
            <div class="dashboard-secret-key mono">{{ selectedNotificationCard.auth_key }}</div>
            <div class="dashboard-secret-meta">
              <div><strong>用户：</strong>{{ selectedNotificationCard.sender_id }}</div>
              <div><strong>用途：</strong>{{ selectedNotificationCard.purpose }}</div>
              <div><strong>过期时间：</strong>{{ selectedNotificationCard.expires_at }}</div>
            </div>
          </div>
        </div>
      </div>
    </Teleport>
  </section>
</template>

<script setup lang="ts">
import { useDashboard } from "../composables/useDashboard";
import Chat from "./Chat.vue";

const {
  services,
  servicesLoading,
  operatingId,
  pendingAction,
  chatModalAgentId,
  chatModalSessionId,
  notificationCards,
  selectedNotificationCard,
  clearingNotifications,
  stats,
  chatModalService,
  llmName,
  runtimeBadgeText,
  startService,
  stopService,
  openChatModal,
  openNotificationKeyModal,
  clearAllNotifications,
  closeChatModal,
  openChatInNewWindow,
  compactId,
  agentAvatarUrl,
  agentInitial,
  statusTone,
  CHAT_ELIGIBLE_SERVICE_TYPES,
} = useDashboard();
</script>

<style scoped lang="scss">
@use "../styles/dashboard" as *;
</style>
