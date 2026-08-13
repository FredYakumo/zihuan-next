<template>
  <section :class="embedded ? 'chat-embedded-wrapper' : 'page chat-page'">
    <div :class="embedded ? 'chat-embedded-inner' : 'chat-page-panel'">
      <section class="panel chat-panel">
        <div v-if="embedded && !agentId" class="chat-toolbar">
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

        <div :class="['chat-layout', { 'chat-layout--history-collapsed': historyCollapsed }]">
          <aside v-if="!historyCollapsed" class="chat-sessions">
            <template v-if="!embedded">
              <div class="chat-service-select-row">
                <t-select
                  v-model="selectedServiceId"
                  class="chat-service-select"
                  :loading="servicesLoading"
                  placeholder="选择 Service"
                >
                  <template #valueDisplay>
                    <div v-if="selectedService" class="chat-service-select-value">
                      <img
                        v-if="agentAvatarUrl(selectedService)"
                        :src="agentAvatarUrl(selectedService)"
                        class="chat-service-select-avatar"
                        alt=""
                      />
                      <span v-else class="chat-service-select-avatar chat-service-select-avatar--fallback">
                        {{ agentInitial(selectedService.name) }}
                      </span>
                      <span class="chat-service-select-value-name">{{ selectedService.name }}</span>
                    </div>
                  </template>
                  <t-option
                    v-for="agent in services.filter((item) => CHAT_ELIGIBLE_SERVICE_TYPES.has(item.agent_type.type))"
                    :key="agent.config_id"
                    :value="agent.config_id"
                    :label="agent.name"
                  >
                    <div class="chat-service-select-option">
                      <img
                        v-if="agentAvatarUrl(agent)"
                        :src="agentAvatarUrl(agent)"
                        class="chat-service-select-avatar"
                        alt=""
                      />
                      <span v-else class="chat-service-select-avatar chat-service-select-avatar--fallback">
                        {{ agentInitial(agent.name) }}
                      </span>
                      <span class="chat-service-select-option-name">{{ agent.name }}</span>
                      <span v-if="agent.runtime.status !== 'running'" class="chat-service-select-status">
                        未运行
                      </span>
                    </div>
                  </t-option>
                </t-select>
                <button
                  class="chat-history-toggle"
                  title="收起历史"
                  aria-label="收起历史"
                  @click="toggleHistory"
                >
                  <MenuFoldIcon />
                </button>
              </div>
            </template>
            <div class="chat-sessions-actions">
              <template v-if="!embedded || agentId">
                <button class="chat-sessions-action" @click="reloadSessions">刷新历史</button>
                <button
                  v-if="isWorkspaceService"
                  class="chat-sessions-action"
                  :disabled="pickingDirectory"
                  @click="pickDirectory"
                >
                  {{ pickingDirectory ? "选择中..." : "打开目录" }}
                </button>
              </template>
              <button
                v-if="embedded"
                class="chat-history-toggle"
                title="收起历史"
                aria-label="收起历史"
                @click="toggleHistory"
              >
                <MenuFoldIcon />
              </button>
            </div>
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

          <div :class="['chat-main', { 'chat-main--history-collapsed': historyCollapsed }]">
            <button
              v-if="historyCollapsed"
              class="chat-history-expand"
              title="展开历史"
              aria-label="展开历史"
              @click="toggleHistory"
            >
              <MenuUnfoldIcon />
            </button>
            <div v-if="isWorkspaceService" class="workspace-path-display">
              <span class="path-label">当前工作目录：</span>
              <span class="path-value" :class="{ 'path-unset': !workspacePath }">
                {{ workspacePath || '未选择工作目录' }}
              </span>
            </div>
            <div class="chat-messages" ref="messagesContainer" @scroll="handleMessagesScroll">
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
                            @click="openLiveToolPreview(
                              liveCall.call_id,
                              classifyToolCall(liveCall.name, liveCall.arguments, liveCall.result),
                            )"
                          />
                          <pre
                            v-if="
                              classifyToolCall(liveCall.name, liveCall.arguments, liveCall.result).type ===
                              'exec_cmd'
                            "
                            class="chat-tool-live-output"
                            :ref="(element) => setLiveOutputElement(liveCall.call_id, element)"
                            @scroll="handleLiveOutputScroll(liveCall.call_id)"
                          >{{ liveExecOutput(liveCall) }}</pre>
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
                      <div class="chat-bubble-footer">
                        <div class="chat-bubble-time">{{ formatChatTime(message.timestamp) }}</div>
                        <div v-if="!message.streaming" class="chat-message-actions chat-message-actions--inside">
                          <t-tooltip content="复制消息">
                            <t-button
                              variant="text"
                              size="small"
                              shape="square"
                              :aria-label="copiedMessageId === message.id ? '已复制' : '复制消息'"
                              @click="copyMessage(message)"
                            >
                              <CheckIcon v-if="copiedMessageId === message.id" />
                              <CopyIcon v-else />
                            </t-button>
                          </t-tooltip>
                        </div>
                      </div>
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
                            @click="openLiveToolPreview(
                              liveCall.call_id,
                              classifyToolCall(liveCall.name, liveCall.arguments, liveCall.result),
                            )"
                          />
                          <pre
                            v-if="
                              classifyToolCall(liveCall.name, liveCall.arguments, liveCall.result).type ===
                              'exec_cmd'
                            "
                            class="chat-tool-live-output"
                            :ref="(element) => setLiveOutputElement(liveCall.call_id, element)"
                            @scroll="handleLiveOutputScroll(liveCall.call_id)"
                          >{{ liveExecOutput(liveCall) }}</pre>
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
                    class="chat-user-message-item"
                  >
                    <div v-if="editingMessage?.messageId === message.id" class="chat-message-edit">
                      <div v-if="editingMessage.imageAttachments.length" class="chat-draft-images">
                        <div v-for="attachment in editingMessage.imageAttachments" :key="attachment.id" class="chat-draft-image">
                          <button class="chat-draft-image-preview" :title="attachment.name" @click="openImagePreview(attachment)">
                            <img :src="attachment.url" :alt="attachment.name" />
                          </button>
                          <span v-if="attachment.uploading" class="chat-draft-image-status">上传中...</span>
                          <span v-else-if="attachment.error" class="chat-draft-image-status chat-draft-image-status--error">
                            {{ attachment.error }}
                          </span>
                          <button class="chat-draft-image-remove" :aria-label="`删除 ${attachment.name}`" @click="removeEditingImageAttachment(attachment.id)">
                            <CloseIcon />
                          </button>
                        </div>
                      </div>
                      <textarea v-model="editingMessage.content" placeholder="输入消息" />
                      <div class="chat-message-edit-actions">
                        <template v-if="supportsMultimodalInput">
                          <input
                            :id="`chat-edit-image-upload-${message.id}`"
                            class="chat-image-upload-input"
                            type="file"
                            accept="image/*"
                            multiple
                            @change="handleEditImageFileSelection"
                          />
                          <label class="chat-message-action-button" :for="`chat-edit-image-upload-${message.id}`" title="添加图片">
                            <ImageAddIcon />
                          </label>
                        </template>
                        <t-button variant="text" size="small" @click="cancelEditingMessage">取消</t-button>
                        <t-button theme="primary" size="small" :disabled="sending" @click="submitEditingMessage">发送</t-button>
                      </div>
                    </div>
                    <template v-else>
                      <div class="chat-bubble" :class="message.role">
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
                        <div class="chat-bubble-footer chat-bubble-footer--user">
                          <div class="chat-bubble-time">{{ formatChatTime(message.timestamp) }}</div>
                          <div class="chat-message-actions chat-message-actions--inside">
                            <t-tooltip content="复制消息">
                              <t-button variant="text" size="small" shape="square" :aria-label="copiedMessageId === message.id ? '已复制' : '复制消息'" @click="copyMessage(message)">
                                <CheckIcon v-if="copiedMessageId === message.id" />
                                <CopyIcon v-else />
                              </t-button>
                            </t-tooltip>
                            <t-tooltip content="编辑并创建新分支">
                              <t-button variant="text" size="small" shape="square" :disabled="sending || message.id.startsWith('local-')" aria-label="编辑消息" @click="startEditingMessage(message)">
                                <EditIcon />
                              </t-button>
                            </t-tooltip>
                            <template v-if="messageBranchMap.has(message.id)">
                              <t-button variant="text" size="small" shape="square" :disabled="sending || messageBranchMap.get(message.id)?.current_index === 0" aria-label="上一版本" @click="switchMessageBranch(message.id, -1)">
                                <ChevronLeftIcon />
                              </t-button>
                              <span class="chat-branch-count">{{ (messageBranchMap.get(message.id)?.current_index ?? 0) + 1 }}/{{ messageBranchMap.get(message.id)?.total ?? 1 }}</span>
                              <t-button variant="text" size="small" shape="square" :disabled="sending || (messageBranchMap.get(message.id)?.current_index ?? 0) + 1 === (messageBranchMap.get(message.id)?.total ?? 1)" aria-label="下一版本" @click="switchMessageBranch(message.id, 1)">
                                <ChevronRightIcon />
                              </t-button>
                            </template>
                          </div>
                        </div>
                      </div>
                    </template>
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
                <div v-if="workspaceChanges.length" class="workspace-change-panel">
                  <div class="workspace-change-panel-header">
                    <strong>文件更改</strong>
                    <span>{{ workspaceChanges.length }} 处待处理</span>
                  </div>
                  <div class="workspace-change-list">
                    <div v-for="change in workspaceChanges" :key="change.change_id" class="workspace-change-row">
                      <button class="workspace-change-summary" @click="openWorkspaceChange(change)">
                        <span class="workspace-change-operation">{{ change.operation }}</span>
                        <span class="workspace-change-path" :title="change.paths.join(' → ')">{{ change.display_path }}</span>
                        <span class="workspace-change-lines">+{{ change.added_lines }} / -{{ change.removed_lines }}</span>
                      </button>
                      <button class="workspace-change-accept" @click="acceptWorkspaceChange(change)">Accept</button>
                      <button class="workspace-change-cancel" @click="cancelWorkspaceChange(change)">Cancel</button>
                    </div>
                  </div>
                </div>
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
                    <template v-if="supportsMultimodalInput">
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
                    </template>
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

                      <button
                        v-if="isWorkspaceService"
                        class="model-chip agents-md-chip"
                        title="管理 AGENTS.md"
                        @click.stop="openAgentsMdDialog"
                      >
                        AGENTS.md
                      </button>

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
                    <t-tooltip v-if="sending" content="停止推理">
                      <t-button theme="danger" shape="square" aria-label="停止推理" @click="stopInference">
                        <StopIcon />
                      </t-button>
                    </t-tooltip>
                    <button v-else class="btn primary" :disabled="!canSend" @click="sendMessage">发送</button>
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
      <div v-if="workspaceChangeDialogOpen" class="workspace-change-overlay" @click.self="closeWorkspaceChange">
        <div class="workspace-change-dialog" role="dialog" aria-modal="true" aria-label="文件更改">
          <aside class="workspace-change-dialog-sidebar">
            <strong>文件更改</strong>
            <div v-for="group in workspaceFileGroups" :key="group.path" class="workspace-change-file-row">
              <button
                class="workspace-change-file"
                :class="{ active: selectedWorkspaceChange?.display_path === group.path }"
                @click="openWorkspaceChange(group.changes[0])"
              >
                <span>{{ group.path }}</span>
                <small>{{ group.changes.length }} 处更改</small>
              </button>
              <div class="workspace-change-file-actions">
                <button class="workspace-change-accept" @click="acceptWorkspaceFile(group.path)">Accept</button>
                <button class="workspace-change-cancel" @click="cancelWorkspaceFile(group.path)">Cancel</button>
              </div>
            </div>
          </aside>
          <section class="workspace-change-dialog-main">
            <header class="workspace-change-dialog-header">
              <strong>{{ selectedWorkspaceChange?.display_path || "文件更改" }}</strong>
              <button aria-label="关闭文件更改" @click="closeWorkspaceChange"><CloseIcon /></button>
            </header>
            <div v-if="selectedWorkspaceChange" class="workspace-change-detail">
              <div class="workspace-change-detail-meta">
                <span>操作：{{ selectedWorkspaceChange.operation }}</span>
                <span>合并：{{ selectedWorkspaceChange.merged_count }} 次</span>
                <span>行数：+{{ selectedWorkspaceChange.added_lines }} / -{{ selectedWorkspaceChange.removed_lines }}</span>
              </div>
              <div class="workspace-change-paths">
                <div v-for="path in selectedWorkspaceChange.paths" :key="path">{{ path }}</div>
              </div>
              <div v-if="selectedWorkspaceChange.diff.length" class="workspace-change-diff">
                <div class="workspace-change-diff-header">
                  <span>更改前</span>
                  <span>更改后</span>
                </div>
                <div
                  v-for="(row, index) in workspaceDiffRows(selectedWorkspaceChange.diff)"
                  :key="index"
                  class="workspace-change-diff-row"
                >
                  <div class="workspace-change-diff-cell workspace-change-diff-cell--removed" :class="{ empty: row.removed === undefined }">
                    <template v-if="row.removed !== undefined"><span>−</span><code>{{ row.removed }}</code></template>
                  </div>
                  <div class="workspace-change-diff-cell workspace-change-diff-cell--added" :class="{ empty: row.added === undefined }">
                    <template v-if="row.added !== undefined"><span>+</span><code>{{ row.added }}</code></template>
                  </div>
                </div>
              </div>
              <p class="workspace-change-note">文件已写入磁盘。Accept 只确认并移除记录，Cancel 会在文件未被外部修改时恢复。</p>
              <p v-if="workspaceChangeError" class="workspace-change-error">{{ workspaceChangeError }}</p>
            </div>
            <footer v-if="selectedWorkspaceChange" class="workspace-change-dialog-actions">
              <button class="workspace-change-accept" @click="acceptWorkspaceChange(selectedWorkspaceChange)">Accept</button>
              <button class="workspace-change-cancel" @click="cancelWorkspaceChange(selectedWorkspaceChange)">Cancel</button>
            </footer>
          </section>
        </div>
      </div>
    </Teleport>

    <Teleport to="body">
      <div v-if="agentsMdDialogOpen" class="agents-md-overlay" @click.self="closeAgentsMdDialog">
        <div class="agents-md-dialog" role="dialog" aria-modal="true" aria-label="AGENTS.md 管理">
          <aside class="agents-md-dialog-sidebar">
            <strong>AGENTS.md</strong>
            <div v-if="!agentsMdEnabled" class="agents-md-disabled-hint">当前未开启 AGENTS.md 读取，可在 Service 配置中开启后按优先级应用。</div>
            <div v-if="!workspacePath" class="agents-md-disabled-hint">未选择工作目录，将使用当前运行目录检测 AGENTS.md。</div>
            <div class="agents-md-toolbar">
              <button class="agents-md-create" :disabled="agentsMdSaving" @click="createAgentsMd">创建 AGENTS.md</button>
              <button class="agents-md-refresh" :disabled="agentsMdLoading" @click="refreshAgentsMd">刷新</button>
            </div>
            <div v-if="agentsMdLoading" class="agents-md-loading">加载中…</div>
            <div v-for="file in agentsMdFiles" :key="file.key" class="agents-md-file-row">
              <div
                class="agents-md-file"
                :class="{ active: agentsMdEditingKey === file.key, applied: agentsMdAppliedKeys.has(file.key) }"
              >
                <div class="agents-md-file-head">
                  <strong>{{ agentsMdLocationLabel(file.key) }}</strong>
                  <span v-if="file.exists" class="agents-md-status agents-md-status--exists">
                    {{ agentsMdAppliedKeys.has(file.key) ? "已应用" : "存在" }}
                  </span>
                  <span v-else class="agents-md-status">未创建</span>
                </div>
                <span class="agents-md-file-path" :title="file.path">{{ file.path }}</span>
              </div>
              <div class="agents-md-file-actions">
                <button v-if="file.exists" class="agents-md-edit" :disabled="agentsMdSaving" @click="selectAgentsMdFile(file)">编辑</button>
                <button v-if="file.exists" class="agents-md-delete" :disabled="agentsMdSaving" @click="deleteAgentsMd(file)">删除</button>
              </div>
            </div>
            <div v-if="agentsMdError" class="agents-md-error">{{ agentsMdError }}</div>
          </aside>
          <section class="agents-md-dialog-main">
            <header class="agents-md-dialog-header">
              <strong>{{ agentsMdEditingKey ? `编辑 ${agentsMdLocationLabel(agentsMdEditingKey)} AGENTS.md` : "AGENTS.md 内容" }}</strong>
              <button aria-label="关闭 AGENTS.md 管理" @click="closeAgentsMdDialog"><CloseIcon /></button>
            </header>
            <div class="agents-md-editor-wrap">
              <div ref="agentsMdLineNumbersRef" class="agents-md-line-numbers">
                <div v-for="n in agentsMdLineCount" :key="n" class="agents-md-line-number">{{ n }}</div>
              </div>
              <textarea
                ref="agentsMdEditorRef"
                v-model="agentsMdEditorContent"
                class="agents-md-editor"
                spellcheck="false"
                wrap="off"
                placeholder="输入 AGENTS.md 内容…"
                @scroll="syncAgentsMdScroll"
              ></textarea>
            </div>
            <footer class="agents-md-dialog-actions">
              <button class="agents-md-save" :disabled="agentsMdSaving || !agentsMdEditingKey" @click="saveAgentsMd">保存</button>
              <button class="agents-md-cancel" :disabled="agentsMdSaving" @click="closeAgentsMdDialog">取消</button>
            </footer>
          </section>
        </div>
      </div>
    </Teleport>

    <Teleport to="body">
      <div v-if="chatErrorDialogMessage" class="chat-error-dialog-overlay" @click.self="closeChatErrorDialog">
        <div class="chat-error-dialog" role="alertdialog" aria-modal="true" aria-labelledby="chat-error-dialog-title">
          <div class="chat-error-dialog-header">
            <strong id="chat-error-dialog-title">推理失败</strong>
            <button aria-label="关闭错误提示" @click="closeChatErrorDialog"><CloseIcon /></button>
          </div>
          <div class="chat-error-dialog-body">{{ chatErrorDialogMessage }}</div>
          <div class="chat-error-dialog-actions">
            <button class="btn primary" @click="closeChatErrorDialog">知道了</button>
          </div>
        </div>
      </div>
    </Teleport>

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
            <template v-else-if="toolPreviewState.kind.type === 'read_file'">
              <FileSearchIcon class="badge-icon" /> 读取文件: {{ toolPreviewState.kind.filename }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'list_dir'">
              <FolderSearchIcon class="badge-icon" /> 列出目录: {{ toolPreviewState.kind.dirname }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'grep'">
              <SearchIcon class="badge-icon" /> Grep: {{ toolPreviewState.kind.pattern }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'rg'">
              <CodeIcon class="badge-icon" /> Rg: {{ toolPreviewState.kind.pattern }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'find_files'">
              <FolderSearchIcon class="badge-icon" /> 查找: {{ toolPreviewState.kind.pattern }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'git_status'">
              <GitBranchIcon class="badge-icon" /> Git: {{ toolPreviewState.kind.branch }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'copy_file'">
              <CopyIcon class="badge-icon" /> 复制: {{ toolPreviewState.kind.src }} → {{ toolPreviewState.kind.dest }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'move_file'">
              <MoveIcon class="badge-icon" /> 移动: {{ toolPreviewState.kind.src }} → {{ toolPreviewState.kind.dest }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'file_info'">
              <InfoCircleIcon class="badge-icon" /> 元数据: {{ toolPreviewState.kind.filename }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'ask_user'">
              <ChatIcon class="badge-icon" /> 询问用户
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'memory_agent'">
              <BookmarkIcon class="badge-icon" /> {{ toolPreviewState.kind.action === 'remember' ? '记录记忆' : '回忆记忆' }}
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'web_search'">
              <InternetIcon class="badge-icon" /> Web Search
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
            <template v-else-if="toolPreviewState.kind.type === 'memory_agent'">
              <div class="tool-preview-info">
                <p>{{ toolPreviewState.kind.action === 'remember' ? '正在记录记忆。' : '正在回忆记忆。' }}</p>
                <pre v-if="toolPreviewState.kind.content">{{ toolPreviewState.kind.content }}</pre>
              </div>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'web_search'">
              <div v-if="toolPreviewState.kind.error" class="tool-preview-error">{{ toolPreviewState.kind.error }}</div>
              <div v-else class="tool-preview-info">
                <p v-if="toolPreviewState.kind.query">搜索: <code>{{ toolPreviewState.kind.query }}</code></p>
                <p v-if="toolPreviewState.kind.url">URL: <code>{{ toolPreviewState.kind.url }}</code></p>
                <p>返回 {{ toolPreviewState.kind.results.length }} 条结果</p>
                <div v-if="toolPreviewState.kind.results.length" class="web-search-results-table">
                  <div v-for="(item, index) in toolPreviewState.kind.results" :key="`${item.url}:${index}`" class="web-search-result-row">
                    <div class="web-search-result-index">{{ index + 1 }}</div>
                    <div class="web-search-result-main">
                      <div class="web-search-result-title-row">
                        <div class="web-search-result-title">{{ item.title }}</div>
                        <span v-if="item.score != null" class="web-search-result-score">Score {{ item.score.toFixed(2) }}</span>
                      </div>
                      <a v-if="item.url" class="web-search-result-url" :href="item.url" target="_blank" rel="noreferrer">{{ item.url }}</a>
                      <div class="web-search-result-content">{{ item.content }}</div>
                    </div>
                  </div>
                </div>
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
              <div class="tool-preview-info">
                <p v-if="toolPreviewState.kind.shell">Shell: {{ toolPreviewState.kind.shell }}</p>
                <p v-if="toolPreviewState.kind.exitCode != null">Exit code: {{ toolPreviewState.kind.exitCode }}</p>
                <p v-if="toolPreviewState.kind.truncated" class="tool-preview-no-result">输出已截断</p>
              </div>
              <template v-if="toolPreviewState.kind.hasResult">
                <div v-if="toolPreviewState.kind.stdout" class="tool-preview-output">
                  <div class="tool-preview-output-label">stdout</div>
                  <pre
                    class="tool-preview-code tool-preview-code--cmd"
                    :ref="toolPreviewState.toolCallId ? (element) => setLiveOutputElement(`${toolPreviewState.toolCallId}:stdout`, element) : undefined"
                    @scroll="toolPreviewState.toolCallId && handleLiveOutputScroll(`${toolPreviewState.toolCallId}:stdout`)"
                  >
                    {{ toolPreviewState.kind.stdout }}
                  </pre>
                </div>
                <div v-if="toolPreviewState.kind.stderr" class="tool-preview-output">
                  <div class="tool-preview-output-label tool-preview-output-label--error">
                    stderr
                  </div>
                  <pre
                    class="tool-preview-code tool-preview-code--cmd tool-preview-code--error"
                    :ref="toolPreviewState.toolCallId ? (element) => setLiveOutputElement(`${toolPreviewState.toolCallId}:stderr`, element) : undefined"
                    @scroll="toolPreviewState.toolCallId && handleLiveOutputScroll(`${toolPreviewState.toolCallId}:stderr`)"
                  >
                    {{ toolPreviewState.kind.stderr }}
                  </pre>
                </div>
              </template>
              <div v-else class="tool-preview-no-result">无结果</div>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'read_file'">
              <div class="tool-preview-info">
                <p v-if="toolPreviewState.kind.startLine != null && toolPreviewState.kind.endLine != null">
                  行范围: {{ toolPreviewState.kind.startLine }}-{{ toolPreviewState.kind.endLine }}
                  <span v-if="toolPreviewState.kind.totalLines != null">（共 {{ toolPreviewState.kind.totalLines }} 行）</span>
                </p>
                <pre v-if="toolPreviewState.kind.content" class="tool-preview-code">{{ toolPreviewState.kind.content }}</pre>
                <p v-if="toolPreviewState.kind.encoding">编码: {{ toolPreviewState.kind.encoding }}</p>
                <div
                  v-if="!toolPreviewState.kind.content && !toolPreviewState.kind.encoding"
                  class="tool-preview-no-result"
                >
                  无内容或工具仍在执行
                </div>
              </div>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'list_dir'">
              <pre v-if="toolPreviewState.kind.tree" class="tool-preview-code">{{ toolPreviewState.kind.tree }}</pre>
              <div v-if="toolPreviewState.kind.entries.length" class="tool-preview-list">
                <div v-for="entry in toolPreviewState.kind.entries" :key="entry.path" class="tool-preview-list-item">
                  <FolderIcon v-if="entry.type === 'directory'" />
                  <FileIcon v-else />
                  <span>{{ entry.name }}</span>
                  <small>{{ entry.type }}</small>
                </div>
              </div>
              <div v-else class="tool-preview-no-result">目录为空或工具仍在执行</div>
              <div v-if="toolPreviewState.kind.truncated" class="tool-preview-no-result">结果已截断</div>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'grep' || toolPreviewState.kind.type === 'rg'">
              <div class="tool-preview-info">命中文件: {{ toolPreviewState.kind.matchedFiles }}，跳过二进制: {{ toolPreviewState.kind.skippedBinary }}</div>
              <div v-if="toolPreviewState.kind.matches.length" class="tool-preview-list">
                <div v-for="match in toolPreviewState.kind.matches" :key="`${match.path}:${match.line}`" class="tool-preview-match">
                  <div class="tool-preview-match-header">{{ match.path }}:{{ match.line }}</div>
                  <pre class="tool-preview-code">{{ match.content }}</pre>
                </div>
              </div>
              <div v-else class="tool-preview-no-result">未找到匹配或工具仍在执行</div>
              <div v-if="toolPreviewState.kind.truncated" class="tool-preview-no-result">
                结果已截断，共 {{ toolPreviewState.kind.totalMatches }} 处匹配
              </div>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'find_files'">
              <div v-if="toolPreviewState.kind.matches.length" class="tool-preview-list">
                <div v-for="match in toolPreviewState.kind.matches" :key="match.path" class="tool-preview-list-item">
                  <strong>{{ match.name }}</strong> <code>{{ match.path }}</code>
                </div>
              </div>
              <div v-if="toolPreviewState.kind.truncated" class="tool-preview-no-result">结果已截断</div>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'git_status'">
              <div class="tool-preview-info">分支: <code>{{ toolPreviewState.kind.branch }}</code></div>
              <div v-if="toolPreviewState.kind.changes.length" class="tool-preview-list">
                <div v-for="change in toolPreviewState.kind.changes" :key="`${change.status}:${change.path}`" class="tool-preview-list-item">
                  <code>{{ change.status }}</code> {{ change.path }}
                </div>
              </div>
              <div v-else class="tool-preview-no-result">工作区干净</div>
              <div v-if="toolPreviewState.kind.truncated" class="tool-preview-no-result">结果已截断</div>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'copy_file' || toolPreviewState.kind.type === 'move_file'">
              <p>源: <code>{{ toolPreviewState.kind.src }}</code></p>
              <p>目标: <code>{{ toolPreviewState.kind.dest }}</code></p>
              <p>覆盖: {{ toolPreviewState.kind.overwritten ? '是' : '否' }}</p>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'file_info'">
              <pre class="tool-preview-code">{{ formatToolPayload(toolPreviewState.kind.metadata) }}</pre>
            </template>
            <template v-else-if="toolPreviewState.kind.type === 'ask_user'">
              <p>{{ toolPreviewState.kind.question }}</p>
            </template>
          </div>
        </div>
      </div>
    </Teleport>
  </section>
</template>

<script setup lang="ts">
import {
  CheckIcon,
  ChevronLeftIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CloseIcon,
  CopyIcon,
  DeleteIcon,
  EditIcon,
  ErrorCircleIcon,
  FileIcon,
  FileSearchIcon,
  FolderIcon,
  FolderSearchIcon,
  GitBranchIcon,
  ImageAddIcon,
  MenuFoldIcon,
  MenuUnfoldIcon,
  CodeIcon,
  SearchIcon,
  InfoCircleIcon,
  MoveIcon,
  ChatIcon,
  BookmarkIcon,
  InternetIcon,
  StopIcon,
} from "tdesign-icons-vue-next";

import { computed, ref, watch } from "vue";

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

const HISTORY_COLLAPSED_KEY = "zihuan.chat.history-collapsed";
const historyCollapsed = ref(!props.embedded && localStorage.getItem(HISTORY_COLLAPSED_KEY) === "1");

function toggleHistory() {
  historyCollapsed.value = !historyCollapsed.value;
}

watch(historyCollapsed, (collapsed) => {
  if (!props.embedded) {
    localStorage.setItem(HISTORY_COLLAPSED_KEY, collapsed ? "1" : "0");
  }
});

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
  chatErrorDialogMessage,
  messagesContainer,
  messages,
  messageBranchMap,
  editingMessage,
  copiedMessageId,
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
  supportsMultimodalInput,
  defaultAgentModelId,
  selectedModelLabel,
  selectedThinkingLabel,
  selectedEffortLabel,
  canSend,
  selectedAgentAvatarUrl,
  selectedAgentAvatarFallback,
  pendingAskUser,
  workspaceChanges,
  workspaceFileGroups,
  selectedWorkspaceChange,
  workspaceChangeDialogOpen,
  workspaceChangeError,
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
  openLiveToolPreview,
  closeToolPreview,
  handleToolPreviewKeydown,
  editHunks,
  toggleLiveToolCall,
  formatToolPayload,
  liveExecOutput,
  formatChatTime,
  renderMessageContent,
  scrollToBottom,
  handleMessagesScroll,
  setLiveOutputElement,
  handleLiveOutputScroll,
  clearChatError,
  closeChatErrorDialog,
  handleTextareaKeydown,
  handleTextareaPaste,
  handleImageFileSelection,
  handleEditImageFileSelection,
  removeDraftImageAttachment,
  removeEditingImageAttachment,
  openImagePreview,
  closeImagePreview,
  handleImagePreviewKeydown,
  toggleAutoCollapseThinking,
  clearPendingAskUser,
  openWorkspaceChange,
  closeWorkspaceChange,
  acceptWorkspaceChange,
  cancelWorkspaceChange,
  acceptWorkspaceFile,
  cancelWorkspaceFile,
  pruneFailedAssistantPlaceholder,
  applyInferenceFailure,
  reloadSessions,
  openSession,
  copyMessage,
  startEditingMessage,
  cancelEditingMessage,
  submitEditingMessage,
  switchMessageBranch,
  pickDirectory,
  startNewSession,
  selectModel,
  selectThinkingType,
  selectReasoningEffort,
  closePickersOnClickOutside,
  removeSession,
  applyStreamEvent,
  sendMessage,
  stopInference,
  submitAskUserAnswer,
  sendMessageWithText,
  load,
  formatTime,
  agentAvatarUrl,
  agentInitial,
  getAvatarDisplayUrl,
  agentsMdDialogOpen,
  agentsMdLoading,
  agentsMdSaving,
  agentsMdFiles,
  agentsMdError,
  agentsMdEditingKey,
  agentsMdEditorContent,
  agentsMdEnabled,
  agentsMdAppliedKeys,
  agentsMdLocationLabel,
  openAgentsMdDialog,
  closeAgentsMdDialog,
  refreshAgentsMd,
  selectAgentsMdFile,
  createAgentsMd,
  saveAgentsMd,
  deleteAgentsMd,
  CHAT_ELIGIBLE_SERVICE_TYPES,
} = useChat(props, emit);

const agentsMdEditorRef = ref<HTMLTextAreaElement | null>(null);
const agentsMdLineNumbersRef = ref<HTMLElement | null>(null);

interface WorkspaceDiffRow {
  removed?: string;
  added?: string;
}

function workspaceDiffRows(diff: { kind: "added" | "removed"; line: string; hunk?: number }[]): WorkspaceDiffRow[] {
  const rows: WorkspaceDiffRow[] = [];
  let start = 0;

  while (start < diff.length) {
    const hunk = diff[start].hunk ?? 0;
    let end = start;
    while (end < diff.length && (diff[end].hunk ?? 0) === hunk) {
      end += 1;
    }

    const removed = diff.slice(start, end).filter((line) => line.kind === "removed");
    const added = diff.slice(start, end).filter((line) => line.kind === "added");
    const rowCount = Math.max(removed.length, added.length);
    for (let index = 0; index < rowCount; index += 1) {
      rows.push({ removed: removed[index]?.line, added: added[index]?.line });
    }

    start = end;
  }

  return rows;
}

const agentsMdLineCount = computed(() => {
  const content = agentsMdEditorContent.value;
  if (!content) {
    return 1;
  }
  return content.split("\n").length;
});

function syncAgentsMdScroll() {
  const editor = agentsMdEditorRef.value;
  const gutter = agentsMdLineNumbersRef.value;
  if (editor && gutter) {
    gutter.scrollTop = editor.scrollTop;
  }
}
</script>

<style scoped lang="scss">
@use "../styles/chat" as *;
</style>
