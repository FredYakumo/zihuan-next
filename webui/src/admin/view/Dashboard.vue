<template>
  <section class="page dashboard-page">
    <div class="page-hero dashboard-hero">
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

    <section v-if="servicesLoading && services.length === 0" class="panel dashboard-flat-panel">
      <div class="dashboard-loading-state" aria-live="polite">
        <span class="dashboard-loading-spinner"></span>
        <span>Service 加载中...</span>
      </div>
    </section>

    <section v-else-if="services.length > 0" class="panel dashboard-flat-panel">
      <div class="dashboard-service-list">
        <article
          v-for="service in services"
          :key="service.config_id"
          class="dashboard-service-row"
        >
          <div class="dashboard-service-header-row">
            <div class="dashboard-service-identity">
              <img
                v-if="agentAvatarUrl(service)"
                :src="agentAvatarUrl(service)"
                alt="service avatar"
                class="dashboard-service-avatar"
              />
              <div v-else class="dashboard-service-avatar dashboard-service-avatar--fallback">
                {{ agentInitial(service.name) }}
              </div>
              <div class="dashboard-service-identity-text">
                <div class="dashboard-service-name-row">
                  <h4>{{ service.name }}</h4>
                  <span class="dashboard-service-tag">{{ service.role_service_type.type }}</span>
                  <span v-if="service.is_default" class="dashboard-service-tag">default</span>
                </div>
                <div class="dashboard-service-status-line">
                  <span class="dashboard-status-dot" :class="statusTone(service.runtime.status)"></span>
                  <span>{{ runtimeBadgeText(service) }}</span>
                  <span class="dashboard-service-status-sep">·</span>
                  <span>{{ service.enabled ? "已启用" : "已停用" }}</span>
                </div>
              </div>
            </div>
            <div class="dashboard-service-actions">
              <button
                v-if="CHAT_ELIGIBLE_SERVICE_TYPES.has(service.role_service_type.type)"
                class="btn primary dashboard-service-row-btn"
                :disabled="service.runtime.status !== 'running' || operatingId === service.config_id"
                @click="openChatModal(service.config_id)"
              >
                对话
              </button>
              <button
                class="btn dashboard-service-row-btn"
                :disabled="service.runtime.status === 'running' || operatingId === service.config_id"
                @click="startService(service.config_id)"
              >
                {{ operatingId === service.config_id && pendingAction === 'start' ? "启动中..." : "启动" }}
              </button>
              <button
                class="btn warn dashboard-service-row-btn"
                :disabled="service.runtime.status !== 'running' || operatingId === service.config_id"
                @click="stopService(service.config_id)"
              >
                {{ operatingId === service.config_id && pendingAction === 'stop' ? "停止中..." : "停止" }}
              </button>
            </div>
          </div>

          <div class="dashboard-service-specs">
            <span class="dashboard-service-spec">
              <span class="dashboard-service-spec-label">Config ID</span>
              <span class="mono">{{ compactId(service.config_id) }}</span>
            </span>
            <span class="dashboard-service-spec">
              <span class="dashboard-service-spec-label">模型</span>
              <span>{{ llmName(service) }}</span>
            </span>
            <span v-if="service.role_service_type.type === 'http_stream'" class="dashboard-service-spec">
              <span class="dashboard-service-spec-label">Bind</span>
              <span class="mono">{{ (service.role_service_type as Record<string, unknown>).bind || '127.0.0.1:18080' }}</span>
            </span>
            <span v-else-if="service.role_service_type.type === 'qq_chat'" class="dashboard-service-spec">
              <span class="dashboard-service-spec-label">Bot QQ</span>
              <span class="mono">{{ service.qq_chat_profile?.bot_user_id || '未知' }}</span>
            </span>
            <span v-else class="dashboard-service-spec">
              <span class="dashboard-service-spec-label">工作模式</span>
              <span>Dashboard Session Workspace</span>
            </span>
          </div>

          <div v-if="service.runtime.last_error" class="dashboard-service-row-footer">
            最近错误：{{ service.runtime.last_error }}
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
              <button class="chat-modal-close" aria-label="关闭" @click="closeChatModal"><CloseIcon /></button>
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
              <button class="chat-modal-close" aria-label="关闭" @click="selectedNotificationCard = null"><CloseIcon /></button>
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
import { CloseIcon } from "tdesign-icons-vue-next";

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
@use "../styles/connections" as *;
@use "../styles/dashboard" as *;
</style>
