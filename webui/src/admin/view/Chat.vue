<template>
  <section :class="embedded ? 'chat-embedded-wrapper' : 'page chat-page'">
    <div :class="embedded ? 'chat-embedded-inner' : 'chat-page-panel'">
      <section class="panel chat-panel">
        <div class="chat-toolbar">
          <div class="chat-agent-picker">
            <div class="chat-agent-picker-title">选择 Service</div>
            <div class="chat-agent-cards">
              <div
                v-if="servicesLoading && services.length === 0"
                class="chat-service-loading"
                aria-live="polite"
              >
                <span class="chat-service-loading-spinner"></span>
                <span>Service 加载中...</span>
              </div>
              <template v-else>
                <button
                  v-for="agent in services.filter((a) => CHAT_ELIGIBLE_SERVICE_TYPES.has(a.agent_type.type))"
                  :key="agent.config_id"
                  class="chat-agent-card"
                  :class="{
                    active: selectedServiceId === agent.config_id,
                    inactive: agent.runtime.status !== 'running',
                  }"
                  @click="selectedServiceId = agent.config_id"
                >
                  <img
                    v-if="agentAvatarUrl(agent)"
                    class="chat-agent-card-avatar"
                    :src="agentAvatarUrl(agent)"
                    alt="agent avatar"
                  />
                  <div v-else class="chat-agent-card-avatar chat-agent-card-avatar--fallback">
                    {{ agentInitial(agent.name) }}
                  </div>
                  <div class="chat-agent-card-meta">
                    <strong>{{ agent.name }}</strong>
                    <span>{{ readableAgentType(agent.agent_type.type) }}</span>
                  </div>
                  <span v-if="agent.runtime.status !== 'running'" class="agent-status-badge">
                    未运行
                  </span>
                </button>
              </template>
            </div>
          </div>
          <button class="btn ghost" @click="reloadSessions">刷新历史</button>
          <button
            v-if="isWorkspaceService"
            class="btn ghost"
            :disabled="pickingDirectory"
            @click="pickDirectory"
          >
            {{ pickingDirectory ? "选择中..." : "打开目录" }}
          </button>
        </div>

        <div class="chat-layout">
          <aside class="chat-sessions">
            <div class="chat-sessions-header">历史</div>
            <template v-for="group in groupedSessions" :key="group.pathKey">
              <div class="chat-session-group-header" :title="group.path ?? undefined">
                <FolderIcon /> {{ group.label }}
              </div>
              <div
                v-for="session in group.sessions"
                :key="session.session_id"
                class="chat-session-item"
                :class="{ active: session.session_id === activeSessionId }"
              >
                <button class="chat-session-main" @click="openSession(session.session_id)">
                  <strong>{{ session.title || session.session_id.slice(0, 8) }}</strong>
                  <span class="muted">{{ formatTime(session.updated_at) }}</span>
                </button>
                <button
                  class="chat-session-delete"
                  title="删除会话"
                  @click.stop="removeSession(session.session_id)"
                >
                  ×
                </button>
              </div>
            </template>
            <div v-if="sessions.length === 0" class="muted">暂无历史会话</div>
          </aside>

          <div class="chat-main">
            <div v-if="isWorkspaceService" class="workspace-path-display">
              <span class="path-label">当前工作目录：</span>
              <span class="path-value" :class="{ 'path-unset': !workspacePath }">
                {{ workspacePath || '未选择工作目录' }}
              </span>
            </div>
            <div class="chat-messages" ref="messagesContainer">
              <div v-if="messages.length === 0" class="empty-state"></div>
              <div
                v-for="group in messageGroups"
                :key="group.id"
                class="chat-bubble-row"
                :class="group.role"
              >
                <img
                  v-if="group.role === 'assistant' && group.avatarUrl"
                  class="chat-message-avatar"
                  :src="group.avatarUrl"
                  alt="bot avatar"
                />
                <div
                  v-else-if="group.role === 'assistant'"
                  class="chat-message-avatar chat-message-avatar--fallback"
                >
                  {{ agentInitial(group.agentName || "Bot") }}
                </div>
                <div v-if="group.role === 'assistant'" class="chat-bubble-col">
                  <div
                    v-for="(message, idx) in group.messages"
                    :key="message.id + '-' + idx"
                    class="chat-message-item"
                  >
                    <div
                      v-if="
                        idx === group.messages.length - 1 &&
                        ((message.liveToolCalls && message.liveToolCalls.length > 0) ||
                          message.toolCalls.length > 0 ||
                          activeToolDetail?.messageId === message.id)
                      "
                      class="chat-tool-above-content"
                    >
                      <div
                        v-if="message.liveToolCalls && message.liveToolCalls.length > 0"
                        class="chat-tool-inline-list"
                      >
                        <div
                          v-for="liveCall in message.liveToolCalls"
                          :key="liveCall.call_id"
                          class="chat-live-tool-wrapper"
                        >
                          <template
                            v-if="
                              classifyToolCall(liveCall.name, liveCall.arguments, liveCall.result).type ===
                              'generic'
                            "
                          >
                            <button
                              class="chat-tool-inline"
                              :class="{ active: expandedLiveToolCalls.has(liveCall.call_id) }"
                              @click="toggleLiveToolCall(liveCall.call_id)"
                            >
                              <span v-if="!liveCall.done" class="live-tool-spinner"></span>
                              <CheckIcon v-else class="live-tool-done-icon" />
                              工具调用: {{ liveCall.name }}
                            </button>
                            <div
                              v-if="expandedLiveToolCalls.has(liveCall.call_id)"
                              class="chat-tool-detail-inline"
                            >
                              <div class="chat-tool-detail-inline-block">
                                <div class="chat-tool-detail-caption">arguments</div>
                                <pre>{{ formatToolPayload(liveCall.arguments) }}</pre>
                              </div>
                              <div v-if="liveCall.done" class="chat-tool-detail-inline-block">
                                <div class="chat-tool-detail-caption">result</div>
                                <pre>{{ liveCall.result || "(空结果)" }}</pre>
                              </div>
                              <div v-else class="chat-tool-detail-inline-block">
                                <div class="chat-tool-detail-caption">result</div>
                                <div class="live-tool-pending">推理中...</div>
                              </div>
                            </div>
                          </template>
                          <ToolCallBadge
                            v-else
                            :kind="classifyToolCall(liveCall.name, liveCall.arguments, liveCall.result)"
                            :loading="!liveCall.done"
                            @click="
                              liveCall.done &&
                              openToolPreview(classifyToolCall(liveCall.name, liveCall.arguments, liveCall.result))
                            "
                          />
                        </div>
                      </div>
                      <div v-if="message.toolCalls.length > 0" class="chat-tool-inline-list">
                        <template v-for="toolCall in message.toolCalls" :key="toolCall.id">
                          <button
                            v-if="
                              classifyToolCall(
                                toolCall.function.name,
                                toolCall.function.arguments,
                                getToolResultText(toolCall.id),
                              ).type === 'generic'
                            "
                            class="chat-tool-inline"
                            :class="{ active: activeToolCallId === toolCall.id }"
                            @click="openToolDetail(message.id, toolCall.id)"
                          >
                            调用工具: {{ toolCall.function.name }}
                          </button>
                          <ToolCallBadge
                            v-else
                            :kind="
                              classifyToolCall(
                                toolCall.function.name,
                                toolCall.function.arguments,
                                getToolResultText(toolCall.id),
                              )
                            "
                            @click="
                              openToolPreview(
                                classifyToolCall(
                                  toolCall.function.name,
                                  toolCall.function.arguments,
                                  getToolResultText(toolCall.id),
                                ),
                              )
                            "
                          />
                        </template>
                      </div>
                      <div
                        v-if="activeToolDetail?.messageId === message.id"
                        class="chat-tool-detail-inline"
                      >
                        <div class="chat-tool-detail-inline-header">
                          <strong>{{ activeToolDetail.toolCall.function.name }}</strong>
                          <button class="chat-tool-detail-inline-close" @click="closeToolDetail">
                            收起
                          </button>
                        </div>
                        <div class="chat-tool-detail-inline-block">
                          <div class="chat-tool-detail-caption">tool_call_id</div>
                          <code>{{ activeToolDetail.toolCall.id }}</code>
                        </div>
                        <div class="chat-tool-detail-inline-block">
                          <div class="chat-tool-detail-caption">arguments</div>
                          <pre>{{ formatToolPayload(activeToolDetail.toolCall.function.arguments) }}</pre>
                        </div>
                        <div class="chat-tool-detail-inline-block">
                          <div class="chat-tool-detail-caption">result</div>
                          <pre>{{ activeToolDetail.result || "(空结果)" }}</pre>
                        </div>
                      </div>
                    </div>
                    <div
                      v-if="message.thinkingContent"
                      class="chat-thinking-block"
                      :class="{ collapsed: !message.thinkingExpanded }"
                    >
                      <button
                        class="chat-thinking-toggle"
                        @click="message.thinkingExpanded = !message.thinkingExpanded"
                      >
                        <span class="chat-thinking-icon">
                          <ChevronDownIcon v-if="message.thinkingExpanded" />
                          <ChevronRightIcon v-else />
                        </span>
                        思考过程
                        <span
                          v-if="message.streaming && message.thinkingExpanded"
                          class="live-tool-spinner"
                        ></span>
                      </button>
                      <div v-if="message.thinkingExpanded" class="chat-thinking-content">
                        {{ message.thinkingContent }}
                      </div>
                    </div>
                    <div
                      v-if="message.content.trim().length > 0 || message.streaming"
                      class="chat-bubble"
                      :class="message.role"
                    >
                      <div
                        class="chat-bubble-content markdown-body"
                        v-html="renderMessageContent(message.content, message.streaming)"
                      ></div>
                      <div class="chat-bubble-time">{{ formatChatTime(message.timestamp) }}</div>
                    </div>
                    <div
                      v-if="
                        idx !== group.messages.length - 1 &&
                        ((message.liveToolCalls && message.liveToolCalls.length > 0) ||
                          message.toolCalls.length > 0 ||
                          activeToolDetail?.messageId === message.id)
                      "
                      class="chat-tool-below-content"
                    >
                      <div
                        v-if="message.liveToolCalls && message.liveToolCalls.length > 0"
                        class="chat-tool-inline-list"
                      >
                        <div
                          v-for="liveCall in message.liveToolCalls"
                          :key="liveCall.call_id"
                          class="chat-live-tool-wrapper"
                        >
                          <template
                            v-if="
                              classifyToolCall(liveCall.name, liveCall.arguments, liveCall.result).type ===
                              'generic'
                            "
                          >
                            <button
                              class="chat-tool-inline"
                              :class="{ active: expandedLiveToolCalls.has(liveCall.call_id) }"
                              @click="toggleLiveToolCall(liveCall.call_id)"
                            >
                              <span v-if="!liveCall.done" class="live-tool-spinner"></span>
                              <CheckIcon v-else class="live-tool-done-icon" />
                              工具调用: {{ liveCall.name }}
                            </button>
                            <div
                              v-if="expandedLiveToolCalls.has(liveCall.call_id)"
                              class="chat-tool-detail-inline"
                            >
                              <div class="chat-tool-detail-inline-block">
                                <div class="chat-tool-detail-caption">arguments</div>
                                <pre>{{ formatToolPayload(liveCall.arguments) }}</pre>
                              </div>
                              <div v-if="liveCall.done" class="chat-tool-detail-inline-block">
                                <div class="chat-tool-detail-caption">result</div>
                                <pre>{{ liveCall.result || "(空结果)" }}</pre>
                              </div>
                              <div v-else class="chat-tool-detail-inline-block">
                                <div class="chat-tool-detail-caption">result</div>
                                <div class="live-tool-pending">推理中...</div>
                              </div>
                            </div>
                          </template>
                          <ToolCallBadge
                            v-else
                            :kind="classifyToolCall(liveCall.name, liveCall.arguments, liveCall.result)"
                            :loading="!liveCall.done"
                            @click="
                              liveCall.done &&
                              openToolPreview(classifyToolCall(liveCall.name, liveCall.arguments, liveCall.result))
                            "
                          />
                        </div>
                      </div>
                      <div v-if="message.toolCalls.length > 0" class="chat-tool-inline-list">
                        <template v-for="toolCall in message.toolCalls" :key="toolCall.id">
                          <button
                            v-if="
                              classifyToolCall(
                                toolCall.function.name,
                                toolCall.function.arguments,
                                getToolResultText(toolCall.id),
                              ).type === 'generic'
                            "
                            class="chat-tool-inline"
                            :class="{ active: activeToolCallId === toolCall.id }"
                            @click="openToolDetail(message.id, toolCall.id)"
                          >
                            调用工具: {{ toolCall.function.name }}
                          </button>
                          <ToolCallBadge
                            v-else
                            :kind="
                              classifyToolCall(
                                toolCall.function.name,
                                toolCall.function.arguments,
                                getToolResultText(toolCall.id),
                              )
                            "
                            @click="
                              openToolPreview(
                                classifyToolCall(
                                  toolCall.function.name,
                                  toolCall.function.arguments,
                                  getToolResultText(toolCall.id),
                                ),
                              )
                            "
                          />
                        </template>
                      </div>
                      <div
                        v-if="activeToolDetail?.messageId === message.id"
                        class="chat-tool-detail-inline"
                      >
                        <div class="chat-tool-detail-inline-header">
                          <strong>{{ activeToolDetail.toolCall.function.name }}</strong>
                          <button class="chat-tool-detail-inline-close" @click="closeToolDetail">
                            收起
                          </button>
                        </div>
                        <div class="chat-tool-detail-inline-block">
                          <div class="chat-tool-detail-caption">tool_call_id</div>
                          <code>{{ activeToolDetail.toolCall.id }}</code>
                        </div>
                        <div class="chat-tool-detail-inline-block">
                          <div class="chat-tool-detail-caption">arguments</div>
                          <pre>{{ formatToolPayload(activeToolDetail.toolCall.function.arguments) }}</pre>
                        </div>
                        <div class="chat-tool-detail-inline-block">
                          <div class="chat-tool-detail-caption">result</div>
                          <pre>{{ activeToolDetail.result || "(空结果)" }}</pre>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
                <div v-if="group.role !== 'assistant'" class="chat-bubble-col">
                  <div
                    v-for="(message, idx) in group.messages"
                    :key="message.id + '-' + idx"
                    class="chat-bubble"
                    :class="message.role"
                  >
                    <div v-if="message.imageAttachments?.length" class="chat-message-images">
                      <button
                        v-for="attachment in message.imageAttachments"
                        :key="attachment.id"
                        class="chat-message-image"
                        :title="attachment.name"
                        @click="openImagePreview(attachment)"
                      >
                        <img :src="attachment.url" :alt="attachment.name" />
                      </button>
                    </div>
                    <div
                      class="chat-bubble-content markdown-body"
                      v-html="renderMessageContent(message.content, message.streaming)"
                    ></div>
                    <div class="chat-bubble-time">{{ formatChatTime(message.timestamp) }}</div>
                  </div>
                </div>
              </div>
            </div>

            <div class="chat-input-area">
              <div v-if="!isChatEligible" class="chat-not-supported">
                <ErrorCircleIcon class="chat-not-supported-icon" />
                <div class="chat-not-supported-title">此 Agent 不支持在 Dashboard 聊天</div>
                <div class="chat-not-supported-desc">请在 QQ 群或 HTTP Stream 端点中使用该 Agent。</div>
              </div>
              <template v-else>
                <div v-if="pendingAskUser" class="ask-user-panel">
                  <div class="ask-user-question">{{ pendingAskUser.question }}</div>
                  <div v-if="pendingAskUser.details" class="ask-user-details">
                    {{ pendingAskUser.details }}
                  </div>
                  <div class="ask-user-row">
                    <input
                      v-model="askUserAnswer"
                      type="text"
                      :placeholder="pendingAskUser.placeholder || '请输入补充信息'"
                      @input="clearChatError"
                      @keydown.enter.prevent="submitAskUserAnswer"
                    />
                    <button
                      class="btn primary"
                      :disabled="!canSubmitAskUser"
                      @click="submitAskUserAnswer"
                    >
                      提交补充信息
                    </button>
                  </div>
                </div>
                <div v-if="draftImageAttachments.length" class="chat-draft-images">
                  <div v-for="attachment in draftImageAttachments" :key="attachment.id" class="chat-draft-image">
                    <button class="chat-draft-image-preview" :title="attachment.name" @click="openImagePreview(attachment)">
                      <img :src="attachment.url" :alt="attachment.name" />
                    </button>
                    <span v-if="attachment.uploading" class="chat-draft-image-status">上传中...</span>
                    <span v-else-if="attachment.error" class="chat-draft-image-status chat-draft-image-status--error">
                      {{ attachment.error }}
                    </span>
                    <button class="chat-draft-image-remove" :aria-label="`删除 ${attachment.name}`" @click="removeDraftImageAttachment(attachment.id)">
                      <CloseIcon />
                    </button>
                  </div>
                </div>
                <textarea
                  v-model="draftMessage"
                  placeholder="输入消息"
                  @keydown.enter="handleTextareaKeydown"
                  @paste="handleTextareaPaste"
                  @input="clearChatError"
                />
                <div class="chat-input-hint">使用 shift + enter 换行</div>
                <div class="chat-input-actions">
                  <button class="btn ghost" @click="startNewSession">新对话</button>
                  <div class="chat-input-right">
                    <input
                      id="chat-image-upload"
                      class="chat-image-upload-input"
                      type="file"
                      accept="image/*"
                      multiple
                      @change="handleImageFileSelection"
                    />
                    <label class="btn ghost chat-image-upload-button" for="chat-image-upload" title="上传图片">
                      <ImageAddIcon />
                    </label>
                    <div v-if="isChatEligible" class="chat-model-bar">
                      <div class="model-picker" :class="{ open: openPicker === 'model' }">
                        <button
                          class="model-chip"
                          @click.stop="openPicker = openPicker === 'model' ? null : 'model'"
                        >
                          {{ selectedModelLabel }}
                          <svg
                            class="chip-chevron"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2.5"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                          >
                            <polyline points="6 9 12 15 18 9" />
                          </svg>
                        </button>
                        <div v-if="openPicker === 'model'" class="model-picker-dropdown">
                          <button
                            class="model-picker-item"
                            :class="{ active: selectedModelId === '' }"
                            @click.stop="selectModel('')"
                          >
                            默认模型
                          </button>
                          <button
                            v-for="model in chatModels"
                            :key="model.config_id"
                            class="model-picker-item"
                            :class="{ active: selectedModelId === model.config_id }"
                            @click.stop="selectModel(model.config_id)"
                          >
                            {{ model.name }}
                          </button>
                        </div>
                      </div>

                      <div class="model-picker" :class="{ open: openPicker === 'thinking' }">
                        <button
                          class="model-chip"
                          @click.stop="openPicker = openPicker === 'thinking' ? null : 'thinking'"
                        >
                          {{ selectedThinkingLabel }}
                          <svg
                            class="chip-chevron"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2.5"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                          >
                            <polyline points="6 9 12 15 18 9" />
                          </svg>
                        </button>
                        <div v-if="openPicker === 'thinking'" class="model-picker-dropdown">
                          <button
                            class="model-picker-item"
                            :class="{ active: selectedThinkingType === '' }"
                            @click.stop="selectThinkingType('')"
                          >
                            默认{{ selectedModelLlmConfig?.thinking_type ? (selectedModelLlmConfig.thinking_type === 'enabled' ? '(启用)' : '(禁用)') : '' }}
                          </button>
                          <button
                            class="model-picker-item"
                            :class="{ active: selectedThinkingType === 'enabled' }"
                            @click.stop="selectThinkingType('enabled')"
                          >
                            启用
                          </button>
                          <button
                            class="model-picker-item"
                            :class="{ active: selectedThinkingType === 'disabled' }"
                            @click.stop="selectThinkingType('disabled')"
                          >
                            禁用
                          </button>
                        </div>
                      </div>

                      <div class="model-picker" :class="{ open: openPicker === 'effort' }">
                        <button
                          class="model-chip"
                          @click.stop="openPicker = openPicker === 'effort' ? null : 'effort'"
                        >
                          {{ selectedEffortLabel }}
                          <svg
                            class="chip-chevron"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2.5"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                          >
                            <polyline points="6 9 12 15 18 9" />
                          </svg>
                        </button>
                        <div v-if="openPicker === 'effort'" class="model-picker-dropdown">
                          <button
                            class="model-picker-item"
                            :class="{ active: selectedReasoningEffort === '' }"
                            @click.stop="selectReasoningEffort('')"
                          >
                            默认{{ selectedModelLlmConfig?.reasoning_effort ? `(${selectedModelLlmConfig.reasoning_effort})` : '' }}
                          </button>
                          <button
                            class="model-picker-item"
                            :class="{ active: selectedReasoningEffort === 'low' }"
                            @click.stop="selectReasoningEffort('low')"
                          >
                            low
                          </button>
                          <button
                            class="model-picker-item"
                            :class="{ active: selectedReasoningEffort === 'medium' }"
                            @click.stop="selectReasoningEffort('medium')"
                          >
                            medium
                          </button>
                          <button
                            class="model-picker-item"
                            :class="{ active: selectedReasoningEffort === 'high' }"
                            @click.stop="selectReasoningEffort('high')"
                          >
                            high
                          </button>
                          <button
                            class="model-picker-item"
                            :class="{ active: selectedReasoningEffort === 'max' }"
                            @click.stop="selectReasoningEffort('max')"
                          >
                            max
                          </button>
                        </div>
                      </div>

                      <div class="model-settings" :class="{ open: openPicker === 'settings' }">
                        <button
                          class="model-chip icon-only"
                          title="模型设置"
                          @click.stop="openPicker = openPicker === 'settings' ? null : 'settings'"
                        >
                          <svg
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                          >
                            <circle cx="12" cy="12" r="3" />
                            <path
                              d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"
                            />
                          </svg>
                        </button>
                        <div
                          v-if="openPicker === 'settings'"
                          class="model-picker-dropdown"
                          style="right: 0; left: auto"
                        >
                          <button
                            class="model-picker-item"
                            :class="{ active: autoCollapseThinking }"
                            @click.stop="toggleAutoCollapseThinking"
                          >
                            自动折叠思考过程
                            <CheckIcon v-if="autoCollapseThinking" class="live-tool-done-icon" />
                          </button>
                        </div>
                      </div>
                    </div>
                    <button class="btn primary" :disabled="sending || !canSend" @click="sendMessage">
                      {{ sending ? "推理中..." : "发送" }}
                    </button>
                  </div>
                </div>
              </template>
              <div v-if="chatErrorMessage" class="chat-error-box" role="alert">
                {{ chatErrorMessage }}
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>

    <Teleport to="body">
      <div
        v-if="imagePreviewAttachment"
        class="chat-image-preview-overlay"
        @click.self="closeImagePreview"
        @keydown="handleImagePreviewKeydown"
      >
        <div class="chat-image-preview-dialog" role="dialog" aria-modal="true" :aria-label="imagePreviewAttachment.name">
          <button class="chat-image-preview-close" aria-label="关闭图片预览" @click="closeImagePreview"><CloseIcon /></button>
          <img :src="imagePreviewAttachment.url" :alt="imagePreviewAttachment.name" />
        </div>
      </div>
    </Teleport>

    <Teleport to="body">
      <div
        v-if="toolPreviewState"
        class="tool-preview-overlay"
        @click.self="closeToolPreview"
        @keydown="handleToolPreviewKeydown"
      >
        <div class="tool-preview-panel">
          <div class="tool-preview-header">
            <template v-if="toolPreviewState.kind.type === 'create_file'">
              <FileIcon class="badge-icon" /> 创建文件: {{ toolPreviewState.kind.filename }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'delete_file'">
              <DeleteIcon class="badge-icon" /> 删除文件: {{ toolPreviewState.kind.filename }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'edit_file'">
              <EditIcon class="badge-icon" /> 编辑文件: {{ toolPreviewState.kind.filename }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'exec_cmd'">
              <span class="cmd-prefix">&gt;</span> {{ toolPreviewState.kind.command }}
            </template>
            <button class="tool-preview-close" aria-label="关闭" @click="closeToolPreview"><CloseIcon /></button>
          </div>
          <div class="tool-preview-body">
            <template v-if="toolPreviewState.kind.type === 'create_file'">
              <pre class="tool-preview-code tool-preview-code--create">
                {{ toolPreviewState.kind.content }}
              </pre>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'delete_file'">
              <div class="tool-preview-info tool-preview-info--delete">
                <p>已删除文件: <code>{{ toolPreviewState.kind.filename }}</code></p>
                <p v-if="toolPreviewState.kind.lineCount != null">
                  共 {{ toolPreviewState.kind.lineCount }} 行
                </p>
              </div>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'edit_file'">
              <div class="tool-preview-diff">
                <div
                  v-for="(hunk, idx) in editHunks(toolPreviewState.kind.edits)"
                  :key="idx"
                  class="tool-preview-hunk"
                >
                  <div
                    v-for="line in hunk.removed"
                    :key="line"
                    class="tool-preview-diff-line tool-preview-diff-line--removed"
                  >
                    <span class="diff-marker">-</span> {{ line }}
                  </div>
                  <div
                    v-for="line in hunk.added"
                    :key="line"
                    class="tool-preview-diff-line tool-preview-diff-line--added"
                  >
                    <span class="diff-marker">+</span> {{ line }}
                  </div>
                </div>
              </div>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'exec_cmd'">
              <template v-if="toolPreviewState.kind.hasResult">
                <div v-if="toolPreviewState.kind.stdout" class="tool-preview-output">
                  <div class="tool-preview-output-label">stdout</div>
                  <pre class="tool-preview-code tool-preview-code--cmd">
                    {{ toolPreviewState.kind.stdout }}
                  </pre>
                </div>
                <div v-if="toolPreviewState.kind.stderr" class="tool-preview-output">
                  <div class="tool-preview-output-label tool-preview-output-label--error">
                    stderr
                  </div>
                  <pre class="tool-preview-code tool-preview-code--cmd tool-preview-code--error">
                    {{ toolPreviewState.kind.stderr }}
                  </pre>
                </div>
              </template>
              <div v-else class="tool-preview-no-result">无结果</div>
            </template>
          </div>
        </div>
      </div>
    </Teleport>
  </section>
</template>

<script setup lang="ts">
import { CheckIcon, ChevronDownIcon, ChevronRightIcon, CloseIcon, DeleteIcon, EditIcon, ErrorCircleIcon, FileIcon, FolderIcon, ImageAddIcon } from "tdesign-icons-vue-next";

import { useChat } from "../composables/useChat";
import ToolCallBadge from "./ToolCallBadge.vue";

const props = defineProps<{
  agentId?: string;
  sessionId?: string;
  embedded?: boolean;
}>();

const emit = defineEmits<{
  (e: "update:sessionId", sessionId: string): void;
}>();

const {
  services,
  servicesLoading,
  sessions,
  activeSessionId,
  selectedServiceId,
  draftMessage,
  draftImageAttachments,
  imagePreviewAttachment,
  workspacePath,
  pickingDirectory,
  sending,
  chatErrorMessage,
  messagesContainer,
  messages,
  activeToolCallId,
  expandedLiveToolCalls,
  llmModels,
  selectedModelId,
  selectedThinkingType,
  selectedReasoningEffort,
  openPicker,
  autoCollapseThinking,
  selectedService,
  selectedServiceType,
  isChatEligible,
  isWorkspaceService,
  groupedSessions,
  chatModels,
  selectedModelLlmConfig,
  defaultAgentModelId,
  selectedModelLabel,
  selectedThinkingLabel,
  selectedEffortLabel,
  canSend,
  selectedAgentAvatarUrl,
  selectedAgentAvatarFallback,
  pendingAskUser,
  askUserAnswer,
  canSubmitAskUser,
  messageGroups,
  activeToolDetail,
  toolPreviewState,
  basename,
  safeParseJson,
  classifyToolCall,
  parseNewConversationCommand,
  messageAvatarUrl,
  readableAgentType,
  toApiMessages,
  applyHistory,
  openToolDetail,
  closeToolDetail,
  getToolResultText,
  openToolPreview,
  closeToolPreview,
  handleToolPreviewKeydown,
  editHunks,
  toggleLiveToolCall,
  formatToolPayload,
  formatChatTime,
  renderMessageContent,
  scrollToBottom,
  clearChatError,
  handleTextareaKeydown,
  handleTextareaPaste,
  handleImageFileSelection,
  removeDraftImageAttachment,
  openImagePreview,
  closeImagePreview,
  handleImagePreviewKeydown,
  toggleAutoCollapseThinking,
  clearPendingAskUser,
  pruneFailedAssistantPlaceholder,
  applyInferenceFailure,
  reloadSessions,
  openSession,
  pickDirectory,
  startNewSession,
  selectModel,
  selectThinkingType,
  selectReasoningEffort,
  closePickersOnClickOutside,
  removeSession,
  applyStreamEvent,
  sendMessage,
  submitAskUserAnswer,
  sendMessageWithText,
  load,
  formatTime,
  agentAvatarUrl,
  agentInitial,
  getAvatarDisplayUrl,
  CHAT_ELIGIBLE_SERVICE_TYPES,
} = useChat(props, emit);
</script>

<style scoped lang="scss">
@use "../styles/chat" as *;
</style>
