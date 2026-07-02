<template>
  <section class="page">
    <div class="page-hero">
      <h2>Service 管理</h2>
      <div class="hero-actions connection-hero-actions">
        <button
          class="btn primary connection-hero-add-btn"
          @click="startCreate"
        >
          +
        </button>
      </div>
    </div>

    <div v-if="showCreatePicker" class="connection-picker-backdrop">
      <div class="connection-picker-dialog service-picker-dialog" @click.stop>
        <div class="connection-picker-header">
          <h3>{{ showCreateForm ? "新建 Service" : "选择 Service 类型" }}</h3>
          <button
            class="btn ghost connection-card-compact-btn"
            @click="closeCreatePicker"
          >
            {{ showCreateForm ? "关闭" : "取消" }}
          </button>
        </div>

        <div v-if="showCreateForm" class="connection-picker-form">
          <div class="form-grid">
            <div class="field">
              <label>名称</label>
              <input v-model="form.name" />
            </div>
            <div class="field">
              <label>类型</label>
              <select v-model="form.type">
                <option value="qq_chat">QQ Chat Agent Service</option>
                <option value="http_stream">HTTP stream service</option>
                <option value="workspace">Workspace Agent Service</option>
              </select>
            </div>

            <div class="field-full status-row">
              <label class="field-check"
                ><input v-model="form.enabled" type="checkbox" />启用</label
              >
              <label class="field-check"
                ><input
                  v-model="form.auto_start"
                  type="checkbox"
                />开机自动启动</label
              >
              <label class="field-check"
                ><input v-model="form.is_default" type="checkbox" />默认 Service</label
              >
            </div>

            <div class="field">
              <label>{{ form.type === 'http_stream' ? '默认模型配置' : '模型配置' }}</label>
              <select v-model="form.llm_ref_id">
                <option value="">请选择</option>
                <option
                  v-for="item in chatModels"
                  :key="item.config_id"
                  :value="item.config_id"
                >
                  {{ item.name }}
                </option>
              </select>
            </div>
            <template v-if="form.type === 'qq_chat'">
              <div class="field">
                <label>数学编程模型</label>
                <select v-model="form.math_programming_llm_ref_id">
                  <option value="">回退主模型</option>
                  <option
                    v-for="item in chatModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>意图分类模型</label>
                <select v-model="form.intent_classification_llm_ref_id">
                  <option value="">回退主模型</option>
                  <option
                    v-for="item in chatModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>自然语言回复模型</label>
                <select v-model="form.natural_language_reply_llm_ref_id">
                  <option value="">请选择</option>
                  <option
                    v-for="item in chatModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field-full">
                <label>自然语言回复 Prompt</label>
                <textarea
                  v-model="form.natural_language_reply_system_prompt"
                  placeholder="可选。专门给自然语言回复模型使用的系统提示词。"
                />
              </div>
              <div class="field">
                <label>文本向量模型</label>
                <select v-model="form.embedding_model_ref_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in embeddingModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>分词 Tokenizer 连接</label>
                <select v-model="form.tokenizer_connection_id">
                  <option value="">不使用（标点分段）</option>
                  <option
                    v-for="item in tokenizerConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Bot Adapter</label>
                <select v-model="form.ims_bot_adapter_connection_id">
                  <option value="">请选择</option>
                  <option
                    v-for="item in botConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Bot Name</label><input v-model="form.bot_name" />
              </div>
              <div class="field-full">
                <label>System Prompt</label>
                <textarea
                  v-model="form.system_prompt"
                  placeholder="可选。会追加在 QQ Chat Agent Service 的通用系统规则后面。"
                />
              </div>
              <div class="field">
                <label>RustFS Connection</label>
                <select v-model="form.rustfs_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in rustfsConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Web Search Engine</label>
                <select v-model="form.web_search_engine_connection_id">
                  <option value="">请选择</option>
                  <option
                    v-for="item in webSearchEngineConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>RDB Connection</label>
                <select v-model="form.rdb_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in taskDbConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Weaviate Image Connection</label>
                <select v-model="form.weaviate_image_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in imageWeaviateConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Weaviate Memory Connection</label>
                <select v-model="form.weaviate_memory_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in memoryWeaviateConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Max Message Length</label
                ><input
                  v-model.number="form.max_message_length"
                  type="number"
                  min="1"
                />
              </div>
              <div class="field">
                <label>Max Steer Count</label>
                <div class="muted">
                  当 Service 还没发出最终回复时，用户继续发消息会被视为"插嘴 /
                  steer"。 这里控制单次活跃回复流程里最多接受多少次插嘴；默认 4
                  次，超出会被丢弃并写入日志。
                </div>
                <input
                  v-model.number="form.max_steer_count"
                  type="number"
                  min="0"
                />
              </div>
              <div class="field">
                <label>配置Service情绪维度</label>
                <div class="muted">
                  Service的情绪可以由一个或者多个维度组成，这些维度共同构成Agent的决策、行为和输出语言风格
                </div>
                <div class="muted" style="margin-top: 6px">
                  当前已配置 {{ form.emotion_dimensions.length }} 个维度。
                </div>
                <button
                  class="btn ghost"
                  type="button"
                  style="margin-top: 6px"
                  @click="openEmotionDimensionsModal"
                >
                  配置情绪维度
                </button>
              </div>
              <div class="field">
                <label>Compact Context Length</label
                ><input
                  v-model.number="form.compact_context_length"
                  type="number"
                  min="0"
                />
              </div>
              <div class="field">
                <label>Rate Limit</label>
                <div class="muted" style="margin-top: 2px">
                  按天/小时/分钟限制调用次数，优先级：用户 &gt; 群组 &gt; 默认。
                </div>
                <button
                  class="btn ghost"
                  type="button"
                  style="margin-top: 6px"
                  @click="openRateLimitModal"
                >
                  编辑 Rate Limit
                </button>
              </div>
              <div class="field">
                <label>Ignore Rules</label>
                <div class="muted" style="margin-top: 2px">
                  命中后仅做消息存储，不回复、不进入推理流程。
                </div>
                <button
                  class="btn ghost"
                  type="button"
                  style="margin-top: 6px"
                  :disabled="Boolean(ignoreRulesDisabledReason)"
                  @click="openIgnoreRulesModal()"
                >
                  管理 Ignore Rules
                </button>
                <div
                  v-if="ignoreRulesDisabledReason"
                  class="muted"
                  style="margin-top: 4px; font-size: 12px"
                >
                  💡 {{ ignoreRulesDisabledReason }}
                </div>
              </div>
            </template>

            <!-- 头像编辑：http_stream 和 workspace 支持 -->
            <template v-if="form.type === 'http_stream' || form.type === 'workspace'">
              <div class="field-full">
                <label>Service 头像</label>
                <div class="avatar-upload-row">
                  <img
                    v-if="form.avatar_url"
                    :src="getAvatarDisplayUrl(form.avatar_url)"
                    alt="Avatar preview"
                    class="avatar-preview"
                  />
                  <div v-else class="avatar-placeholder">
                    {{ form.name ? form.name.slice(0, 1).toUpperCase() : 'A' }}
                  </div>
                  <div class="avatar-actions">
                    <input
                      ref="createAvatarFileInput"
                      type="file"
                      accept="image/*"
                      style="display: none"
                      @change="handleAvatarFileSelect"
                    />
                    <button
                      type="button"
                      class="btn ghost"
                      @click="$refs.createAvatarFileInput?.click()"
                    >
                      {{ form.avatar_url ? '更换头像' : '上传头像' }}
                    </button>
                    <button
                      v-if="form.avatar_url"
                      type="button"
                      class="btn warn"
                      @click="clearAvatar"
                    >
                      删除
                    </button>
                  </div>
                </div>
                <input
                  v-model="form.avatar_url"
                  placeholder="头像 URL（可选，或直接上传图片）"
                  style="margin-top: 8px"
                />
              </div>
            </template>

            <template v-if="form.type === 'http_stream'">
              <div class="field">
                <label>Bind</label
                ><input
                  v-model="form.http_bind"
                  placeholder="127.0.0.1:18080"
                />
              </div>
              <div class="field">
                <label>API Key</label><input v-model="form.http_api_key" />
              </div>
              <div class="field">
                <label>Web Search Engine</label>
                <select v-model="form.http_web_search_engine_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in webSearchEngineConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Task DB Connection</label>
                <select v-model="form.task_db_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in taskDbConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
                <div
                  v-if="!form.task_db_connection_id"
                  class="muted"
                  style="margin-top: 4px"
                >
                  💡
                  未配置关系数据库连接时，任务记录仅在内存中保存，重启服务后会丢失。
                  如需持久化，请在
                  <a href="#/connections" style="color: var(--primary)"
                    >连接管理</a
                  >
                  中新建 MySQL 或 SQLite 连接。
                </div>
              </div>
              <div class="field">
                <label>Memory Embedding Model</label>
                <select v-model="form.http_embedding_model_ref_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in embeddingModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Weaviate Memory Connection</label>
                <select v-model="form.http_weaviate_memory_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in memoryWeaviateConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
            </template>

          </div>

          <div v-if="currentDefaultTools.length > 0" class="editor-card" style="margin-top: 12px">
            <div class="split-header">
              <div>
                <h3>默认工具</h3>
              </div>
            </div>
            <div class="default-tools-table-wrap" style="margin-top: 10px">
              <div class="default-tools-search">
                <input
                  v-model="defaultToolSearchQuery"
                  type="text"
                  placeholder="搜索工具名称、ID 或说明"
                  class="default-tools-search-input"
                />
                <button
                  v-if="defaultToolSearchQuery"
                  class="btn ghost default-tools-search-clear"
                  @click="defaultToolSearchQuery = ''"
                >
                  清空
                </button>
              </div>
              <div v-if="filteredDefaultTools.length === 0" class="empty-state" style="padding: 16px">
                没有匹配的工具。
              </div>
              <table v-else class="default-tools-table">
                <thead>
                  <tr>
                    <th class="dt-col-name">工具名称</th>
                    <th class="dt-col-id">工具 ID</th>
                    <th class="dt-col-desc">说明</th>
                    <th class="dt-col-enable">启用</th>
                    <th class="dt-col-edit">编辑</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="tool in filteredDefaultTools" :key="tool.id">
                    <td class="dt-cell-name"><strong>{{ tool.label }}</strong></td>
                    <td class="dt-cell-id mono">{{ tool.id }}</td>
                    <td class="dt-cell-desc"><span class="muted">{{ tool.description }}</span></td>
                    <td class="dt-cell-enable">
                      <input v-model="form.default_tools_enabled[tool.id]" type="checkbox" />
                    </td>
                    <td class="dt-cell-edit">
                      <button
                        class="btn ghost connection-card-compact-btn"
                        @click="openDefaultToolEditModal(tool.id)"
                      >
                        编辑
                      </button>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
          <div v-if="form.type === 'qq_chat'" class="editor-card" style="margin-top: 12px">
            <div class="field-full">
              <label>达到调用上限回文（可选）</label>
              <textarea
                v-model="form.tool_session_limit_message"
                placeholder="当前工具已经达到 [{limit_scope}]的调用次数限制，但是你不要把这个直接当成自然语言输出给用户，仅当用户问及详细原因时，再告知"
              />
            </div>
            <div class="muted" style="margin-top: 4px">
              留空则使用默认提示。可用 {limit_scope} 占位符表示限制范围（会替换为"单次会话"或"用户"）。
            </div>
          </div>

<!-- 头像编辑：http_stream 和 workspace 支持 -->
            <template v-if="form.type === 'http_stream' || form.type === 'workspace'">
              <div class="field-full">
                <label>Service 头像</label>
                <div class="avatar-upload-row">
                  <img
                    v-if="form.avatar_url"
                    :src="getAvatarDisplayUrl(form.avatar_url)"
                    alt="Avatar preview"
                    class="avatar-preview"
                  />
                  <div v-else class="avatar-placeholder">
                    {{ form.name ? form.name.slice(0, 1).toUpperCase() : 'A' }}
                  </div>
                  <div class="avatar-actions">
                    <input
                      ref="avatarFileInput"
                      type="file"
                      accept="image/*"
                      style="display: none"
                      @change="handleAvatarFileSelect"
                    />
                    <button
                      type="button"
                      class="btn ghost"
                      @click="$refs.avatarFileInput?.click()"
                    >
                      {{ form.avatar_url ? '更换头像' : '上传头像' }}
                    </button>
                    <button
                      v-if="form.avatar_url"
                      type="button"
                      class="btn warn"
                      @click="clearAvatar"
                    >
                      删除
                    </button>
                  </div>
                </div>
                <input
                  v-model="form.avatar_url"
                  placeholder="头像 URL（可选，或直接上传图片）"
                  style="margin-top: 8px"
                />
              </div>
            </template>

          <div class="editor-card" style="margin-top: 18px">
            <div class="split-header">
              <div>
                <h3>工具配置</h3>
              </div>
              <button class="btn ghost" @click="addTool">新增工具</button>
            </div>
            <div class="list" style="margin-top: 14px">
              <div v-if="form.tools.length === 0" class="empty-state">
                还没有配置工具。
              </div>
              <div
                v-for="(tool, index) in form.tools"
                :key="tool.id"
                class="tool-block"
              >
                <div class="split-header">
                  <strong>工具 {{ index + 1 }}</strong>
                  <button class="btn warn" @click="removeTool(index)">
                    移除
                  </button>
                </div>
                <div class="form-grid">
                  <div class="field">
                    <label>ID</label><input v-model="tool.id" />
                  </div>
                  <div class="field">
                    <label>名称</label><input v-model="tool.name" />
                  </div>
                  <div class="field-full">
                    <label>描述</label><input v-model="tool.description" />
                  </div>
                  <div class="field">
                    <label>运行时长</label>
                    <select v-model="tool.runDuration">
                      <option value="Short">Short（短时）</option>
                      <option value="Long">Long（长时）</option>
                    </select>
                  </div>
                  <div class="field">
                    <label>目标类型</label>
                    <select
                      v-model="tool.targetType"
                      @change="handleToolTargetTypeChange(tool)"
                    >
                      <option value="workflow_set">workflow_set</option>
                      <option value="file_path">file_path</option>
                      <option value="inline_graph">inline_graph</option>
                    </select>
                  </div>
                  <div class="field field-check">
                    <input v-model="tool.enabled" type="checkbox" />启用该工具
                  </div>
                  <div
                    v-if="form.type === 'qq_chat' && tool.enabled"
                    class="field"
                  >
                    <label>单次会话调用上限</label>
                    <input
                      v-model.number="form.tool_session_call_limits[tool.name]"
                      type="number"
                      min="0"
                      placeholder="不限制"
                    />
                    <div class="muted" style="font-size: 12px">0 或留空表示不限制</div>
                  </div>
                  <div
                    v-if="tool.targetType === 'workflow_set'"
                    class="field-full"
                  >
                    <label>Workflow Set 名称</label>
                    <select
                      v-model="tool.workflowName"
                      @change="applyWorkflowSetMetadata(tool)"
                    >
                      <option value="">请选择</option>
                      <option
                        v-for="workflow in workflows"
                        :key="workflow.name"
                        :value="workflow.name"
                      >
                        {{ workflow.display_name || workflow.name }}
                      </option>
                    </select>
                  </div>
                  <div
                    v-else-if="tool.targetType === 'file_path'"
                    class="field-full"
                  >
                    <label>文件路径</label>
                    <input
                      v-model="tool.filePath"
                      placeholder="workflow_set/demo.json"
                    />
                  </div>
                  <div v-else class="field-full">
                    <label>Inline Graph JSON</label>
                    <textarea v-model="tool.inlineGraphJson" />
                  </div>
                  <div class="field-full">
                    <div
                      style="
                        display: flex;
                        align-items: center;
                        justify-content: space-between;
                        margin-bottom: 4px;
                      "
                    >
                      <label style="margin-bottom: 0">Parameters JSON</label>
                      <button
                        v-if="
                          tool.targetType === 'workflow_set' &&
                          tool.workflowName
                        "
                        class="btn ghost"
                        style="padding: 2px 10px; font-size: 12px"
                        :disabled="syncingToolIndex === index"
                        @click="syncToolFromGraph(tool, index)"
                      >
                        {{
                          syncingToolIndex === index
                            ? "同步中…"
                            : "从节点图更新"
                        }}
                      </button>
                    </div>
                    <textarea v-model="tool.parametersJson" />
                  </div>
                  <div class="field-full">
                    <label>Outputs JSON</label>
                    <textarea v-model="tool.outputsJson" />
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div class="panel-actions connection-picker-form-actions">
            <button class="btn ghost" @click="showCreateForm = false">
              返回
            </button>
            <button class="btn primary" @click="submitForm">创建 Service</button>
          </div>
        </div>

        <div v-else class="connection-picker-grid">
          <button
            v-for="type in serviceTypes"
            :key="type.value"
            class="connection-picker-option"
            @click="pickCreateType(type.value)"
          >
            <strong>{{ type.label }}</strong>
            <span>{{ type.hint }}</span>
          </button>
        </div>
      </div>
    </div>

    
    <div v-if="showDefaultToolEditModal" class="service-edit-modal-backdrop" style="z-index: 70" @click.stop>
      <div class="service-edit-modal default-tool-edit-modal" @click.stop>
        <div class="service-edit-modal-header">
          <h3 style="margin: 0">编辑默认工具</h3>
          <button class="btn ghost" @click="closeDefaultToolEditModal">关闭</button>
        </div>
        <div class="service-edit-modal-body">
          <div class="editor-card">
            <div class="form-grid">
              <div class="field-full">
                <label>工具</label>
                <div class="muted">
                  {{ currentEditingDefaultTool?.label }} ({{ currentEditingDefaultTool?.id }})
                </div>
              </div>
              <div class="field-full field-check">
                <label>
                  <input v-model="defaultToolEditDraft.enabled" type="checkbox" />
                  启用该工具
                </label>
              </div>
              <div class="field">
                <label>单次会话调用上限</label>
                <input
                  v-model.number="defaultToolEditDraft.callLimit"
                  type="number"
                  min="0"
                  placeholder="不限制"
                />
                <div class="muted" style="font-size: 12px; margin-top: 4px">0 或留空表示不限制</div>
              </div>
              <div
                v-if="form.type === 'qq_chat' && editingDefaultToolId === 'image_understand'"
                class="field-full"
              >
                <label>图片理解模型</label>
                <select v-model="defaultToolEditDraft.imageUnderstandLlmRefId">
                  <option value="">默认使用主模型</option>
                  <option
                    v-for="item in multimodalChatModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
                <div class="muted" style="margin-top: 4px">
                  image_understand 默认使用 Service 主模型；这里只有支持多模态的模型可选。
                </div>
                <div
                  v-if="form.llm_ref_id && !mainChatModelSupportsMultimodal && !defaultToolEditDraft.imageUnderstandLlmRefId"
                  class="muted"
                  style="margin-top: 4px; color: #ffb36b"
                >
                  当前主模型不支持多模态，启用 image_understand 时必须在这里指定一个支持多模态的模型。
                </div>
              </div>
            </div>
          </div>
        </div>
        <div class="service-edit-modal-footer">
          <button class="btn ghost" @click="closeDefaultToolEditModal">取消</button>
          <button class="btn primary" @click="confirmDefaultToolEdit">保存</button>
        </div>
      </div>
    </div>

    <!-- 编辑 Service 模态框 -->
    <div v-if="showEditModal" class="service-edit-modal-backdrop" @click.stop>
      <div class="service-edit-modal" @click.stop>
        <div class="service-edit-modal-header">
          <div class="connection-card-badges">
            <span class="badge">{{ form.type }}</span>
            <span class="badge" :class="form.enabled ? 'success' : ''">{{
              form.enabled ? "已启用" : "已停用"
            }}</span>
            <span v-if="form.is_default" class="badge">default</span>
          </div>
          <h3 style="margin: 0">{{ form.name || "编辑 Service" }}</h3>
        </div>

        <div class="service-edit-modal-body">
          <div class="form-grid">
            <div class="field">
              <label>名称</label>
              <input v-model="form.name" />
            </div>
            <div class="field">
              <label>类型</label>
              <select v-model="form.type">
                <option value="qq_chat">QQ Chat Agent Service</option>
                <option value="http_stream">HTTP stream service</option>
                <option value="workspace">Workspace Agent Service</option>
              </select>
            </div>

            <div class="field-full status-row">
              <label class="field-check"
                ><input v-model="form.enabled" type="checkbox" />启用</label
              >
              <label class="field-check"
                ><input
                  v-model="form.auto_start"
                  type="checkbox"
                />开机自动启动</label
              >
              <label class="field-check"
                ><input v-model="form.is_default" type="checkbox" />默认 Service</label
              >
            </div>

            <div class="field">
              <label>{{ form.type === 'http_stream' ? '默认模型配置' : '模型配置' }}</label>
              <select v-model="form.llm_ref_id">
                <option value="">请选择</option>
                <option
                  v-for="item in chatModels"
                  :key="item.config_id"
                  :value="item.config_id"
                >
                  {{ item.name }}
                </option>
              </select>
            </div>
            <template v-if="form.type === 'qq_chat'">
              <div class="field">
                <label>数学编程模型</label>
                <select v-model="form.math_programming_llm_ref_id">
                  <option value="">回退主模型</option>
                  <option
                    v-for="item in chatModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>意图分类模型</label>
                <select v-model="form.intent_classification_llm_ref_id">
                  <option value="">回退主模型</option>
                  <option
                    v-for="item in chatModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>自然语言回复模型</label>
                <select v-model="form.natural_language_reply_llm_ref_id">
                  <option value="">请选择</option>
                  <option
                    v-for="item in chatModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field-full">
                <label>自然语言回复 Prompt</label>
                <textarea
                  v-model="form.natural_language_reply_system_prompt"
                  placeholder="可选。专门给自然语言回复模型使用的系统提示词。"
                  style="min-height: 100px"
                />
              </div>
              <div class="field">
                <label>文本向量模型</label>
                <select v-model="form.embedding_model_ref_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in embeddingModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>分词 Tokenizer 连接</label>
                <select v-model="form.tokenizer_connection_id">
                  <option value="">不使用（标点分段）</option>
                  <option
                    v-for="item in tokenizerConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Bot Adapter</label>
                <select v-model="form.ims_bot_adapter_connection_id">
                  <option value="">请选择</option>
                  <option
                    v-for="item in botConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Bot Name</label><input v-model="form.bot_name" />
              </div>
              <div class="field-full">
                <label>System Prompt</label>
                <textarea
                  v-model="form.system_prompt"
                  placeholder="可选。会追加在 QQ Chat Agent Service 的通用系统规则后面。"
                  style="min-height: 100px"
                />
              </div>
              <div class="field">
                <label>RustFS Connection</label>
                <select v-model="form.rustfs_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in rustfsConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Web Search Engine</label>
                <select v-model="form.web_search_engine_connection_id">
                  <option value="">请选择</option>
                  <option
                    v-for="item in webSearchEngineConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>RDB Connection</label>
                <select v-model="form.rdb_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in taskDbConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Weaviate Image Connection</label>
                <select v-model="form.weaviate_image_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in imageWeaviateConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Weaviate Memory Connection</label>
                <select v-model="form.weaviate_memory_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in memoryWeaviateConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Max Message Length</label
                ><input
                  v-model.number="form.max_message_length"
                  type="number"
                  min="1"
                />
              </div>
              <div class="field">
                <label>Max Steer Count</label>
                <div class="muted">
                  当 Service 还没发出最终回复时，用户继续发消息会被视为"插嘴 /
                  steer"。 这里控制单次活跃回复流程里最多接受多少次插嘴；默认 4
                  次，超出的消息会被丢弃。
                </div>
                <input
                  v-model.number="form.max_steer_count"
                  type="number"
                  min="0"
                />
              </div>
              <div class="field">
                <label>配置Service情绪维度</label>
                <div class="muted">
                  Service的情绪可以由一个或者多个维度组成，这些维度共同构成Agent的决策、行为和输出语言风格。
                </div>
                <button
                  class="btn ghost"
                  type="button"
                  style="margin-top: 6px"
                  @click="openEmotionDimensionsModal"
                >
                  管理情绪维度
                </button>
              </div>
              <div class="field">
                <label>Compact Context Length</label
                ><input
                  v-model.number="form.compact_context_length"
                  type="number"
                  min="0"
                />
              </div>
              <div class="field">
                <label>Rate Limit</label>
                <div class="muted" style="margin-top: 2px">
                  按天/小时/分钟限制调用次数，优先级：用户 &gt; 群组 &gt; 默认。
                </div>
                <button
                  class="btn ghost"
                  type="button"
                  style="margin-top: 6px"
                  @click="openRateLimitModal"
                >
                  编辑 Rate Limit
                </button>
              </div>
              <div class="field">
                <label>Ignore Rules</label>
                <div class="muted" style="margin-top: 2px">
                  命中后仅做消息存储，不回复、不进入推理流程。
                </div>
                <button
                  class="btn ghost"
                  type="button"
                  style="margin-top: 6px"
                  :disabled="Boolean(ignoreRulesDisabledReason)"
                  @click="openIgnoreRulesModal()"
                >
                  管理 Ignore Rules
                </button>
                <div
                  v-if="ignoreRulesDisabledReason"
                  class="muted"
                  style="margin-top: 4px; font-size: 12px"
                >
                  💡 {{ ignoreRulesDisabledReason }}
                </div>
              </div>
            </template>

            <!-- 头像编辑：http_stream 和 workspace 支持 -->
            <template v-if="form.type === 'http_stream' || form.type === 'workspace'">
              <div class="field-full">
                <label>Service 头像</label>
                <div class="avatar-upload-row">
                  <img
                    v-if="form.avatar_url"
                    :src="getAvatarDisplayUrl(form.avatar_url)"
                    alt="Avatar preview"
                    class="avatar-preview"
                  />
                  <div v-else class="avatar-placeholder">
                    {{ form.name ? form.name.slice(0, 1).toUpperCase() : 'A' }}
                  </div>
                  <div class="avatar-actions">
                    <input
                      ref="avatarFileInput"
                      type="file"
                      accept="image/*"
                      style="display: none"
                      @change="handleAvatarFileSelect"
                    />
                    <button
                      type="button"
                      class="btn ghost"
                      @click="$refs.avatarFileInput?.click()"
                    >
                      {{ form.avatar_url ? '更换头像' : '上传头像' }}
                    </button>
                    <button
                      v-if="form.avatar_url"
                      type="button"
                      class="btn warn"
                      @click="clearAvatar"
                    >
                      删除
                    </button>
                  </div>
                </div>
                <input
                  v-model="form.avatar_url"
                  placeholder="头像 URL（可选，或直接上传图片）"
                  style="margin-top: 8px"
                />
              </div>
            </template>

            <template v-if="form.type === 'http_stream'">
              <div class="field">
                <label>Bind</label
                ><input
                  v-model="form.http_bind"
                  placeholder="127.0.0.1:18080"
                />
              </div>
              <div class="field">
                <label>API Key</label><input v-model="form.http_api_key" />
              </div>
              <div class="field">
                <label>Web Search Engine</label>
                <select v-model="form.http_web_search_engine_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in webSearchEngineConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Task DB Connection</label>
                <select v-model="form.task_db_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in taskDbConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
                <div
                  v-if="!form.task_db_connection_id"
                  class="muted"
                  style="margin-top: 4px"
                >
                  💡
                  未配置关系数据库连接时，任务记录仅在内存中保存，重启服务后会丢失。
                  如需持久化，请在
                  <a href="#/connections" style="color: var(--primary)"
                    >连接管理</a
                  >
                  中新建 MySQL 或 SQLite 连接。
                </div>
              </div>
              <div class="field">
                <label>Memory Embedding Model</label>
                <select v-model="form.http_embedding_model_ref_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in embeddingModels"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>Weaviate Memory Connection</label>
                <select v-model="form.http_weaviate_memory_connection_id">
                  <option value="">不使用</option>
                  <option
                    v-for="item in memoryWeaviateConnections"
                    :key="item.config_id"
                    :value="item.config_id"
                  >
                    {{ item.name }}
                  </option>
                </select>
              </div>

              </template>


          </div>

          <!-- 默认工具表格 -->
          <div class="editor-card" style="margin-top: 12px">
            <div class="split-header">
              <div>
                <h3>默认工具</h3>
              </div>
            </div>
            <div class="default-tools-table-wrap" style="margin-top: 10px">
                <div class="default-tools-search">
                  <input
                    v-model="defaultToolSearchQuery"
                    type="text"
                    placeholder="搜索工具名称、ID 或说明"
                    class="default-tools-search-input"
                  />
                  <button
                    v-if="defaultToolSearchQuery"
                    class="btn ghost default-tools-search-clear"
                    @click="defaultToolSearchQuery = ''"
                  >
                    清空
                  </button>
                </div>
                <div v-if="filteredDefaultTools.length === 0" class="empty-state" style="padding: 16px">
                  没有匹配的工具。
                </div>
                <table v-else class="default-tools-table">
                  <thead>
                    <tr>
                      <th class="dt-col-name">工具名称</th>
                      <th class="dt-col-id">工具 ID</th>
                      <th class="dt-col-desc">说明</th>
                      <th class="dt-col-enable">启用</th>
                      <th class="dt-col-edit">编辑</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="tool in filteredDefaultTools"
                      :key="tool.id"
                    >
                      <td class="dt-cell-name"><strong>{{ tool.label }}</strong></td>
                      <td class="dt-cell-id mono">{{ tool.id }}</td>
                      <td class="dt-cell-desc"><span class="muted">{{ tool.description }}</span></td>
                      <td class="dt-cell-enable">
                        <input
                          v-model="form.default_tools_enabled[tool.id]"
                          type="checkbox"
                        />
                      </td>
                      <td class="dt-cell-edit">
                        <button
                          class="btn ghost connection-card-compact-btn"
                          @click="openDefaultToolEditModal(tool.id)"
                        >
                          编辑
                        </button>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
          </div>
          <div v-if="form.type === 'qq_chat'" class="editor-card" style="margin-top: 12px">
            <div class="field-full">
              <label>达到调用上限回文（可选）</label>
              <textarea
                v-model="form.tool_session_limit_message"
                placeholder="当前工具已经达到 [{limit_scope}]的调用次数限制，但是你不要把这个直接当成自然语言输出给用户，仅当用户问及详细原因时，再告知"
              />
            </div>
            <div class="muted" style="margin-top: 4px">
              留空则使用默认提示。可用 {limit_scope} 占位符表示限制范围（会替换为"单次会话"或"用户"）。
            </div>
          </div>

          <div class="editor-card" style="margin-top: 18px">
            <div class="split-header">
              <div>
                <h3>工具配置</h3>
              </div>
              <button class="btn ghost" @click="addTool">新增工具</button>
            </div>
            <div class="list" style="margin-top: 14px">
              <div v-if="form.tools.length === 0" class="empty-state">
                还没有配置工具。
              </div>
              <div
                v-for="(tool, index) in form.tools"
                :key="tool.id"
                class="tool-block"
              >
                <div class="split-header">
                  <strong>工具 {{ index + 1 }}</strong>
                  <button class="btn warn" @click="removeTool(index)">
                    移除
                  </button>
                </div>
                <div class="form-grid">
                  <div class="field">
                    <label>ID</label><input v-model="tool.id" />
                  </div>
                  <div class="field">
                    <label>名称</label><input v-model="tool.name" />
                  </div>
                  <div class="field-full">
                    <label>描述</label><input v-model="tool.description" />
                  </div>
                  <div class="field">
                    <label>运行时长</label>
                    <select v-model="tool.runDuration">
                      <option value="Short">Short（短时）</option>
                      <option value="Long">Long（长时）</option>
                    </select>
                  </div>
                  <div class="field">
                    <label>目标类型</label>
                    <select
                      v-model="tool.targetType"
                      @change="handleToolTargetTypeChange(tool)"
                    >
                      <option value="workflow_set">workflow_set</option>
                      <option value="file_path">file_path</option>
                      <option value="inline_graph">inline_graph</option>
                    </select>
                  </div>
                  <div class="field field-check">
                    <input v-model="tool.enabled" type="checkbox" />启用该工具
                  </div>
                  <div
                    v-if="form.type === 'qq_chat' && tool.enabled"
                    class="field"
                  >
                    <label>单次会话调用上限</label>
                    <input
                      v-model.number="form.tool_session_call_limits[tool.name]"
                      type="number"
                      min="0"
                      placeholder="不限制"
                    />
                    <div class="muted" style="font-size: 12px">0 或留空表示不限制</div>
                  </div>
                  <div
                    v-if="tool.targetType === 'workflow_set'"
                    class="field-full"
                  >
                    <label>Workflow Set 名称</label>
                    <select
                      v-model="tool.workflowName"
                      @change="applyWorkflowSetMetadata(tool)"
                    >
                      <option value="">请选择</option>
                      <option
                        v-for="workflow in workflows"
                        :key="workflow.name"
                        :value="workflow.name"
                      >
                        {{ workflow.display_name || workflow.name }}
                      </option>
                    </select>
                  </div>
                  <div
                    v-else-if="tool.targetType === 'file_path'"
                    class="field-full"
                  >
                    <label>文件路径</label>
                    <input
                      v-model="tool.filePath"
                      placeholder="workflow_set/demo.json"
                    />
                  </div>
                  <div v-else class="field-full">
                    <label>Inline Graph JSON</label>
                    <textarea v-model="tool.inlineGraphJson" />
                  </div>
                  <div class="field-full">
                    <div
                      style="
                        display: flex;
                        align-items: center;
                        justify-content: space-between;
                        margin-bottom: 4px;
                      "
                    >
                      <label style="margin-bottom: 0">Parameters JSON</label>
                      <button
                        v-if="
                          tool.targetType === 'workflow_set' &&
                          tool.workflowName
                        "
                        class="btn ghost"
                        style="padding: 2px 10px; font-size: 12px"
                        :disabled="syncingToolIndex === index"
                        @click="syncToolFromGraph(tool, index)"
                      >
                        {{
                          syncingToolIndex === index
                            ? "同步中…"
                            : "从节点图更新"
                        }}
                      </button>
                    </div>
                    <textarea v-model="tool.parametersJson" />
                  </div>
                  <div class="field-full">
                    <label>Outputs JSON</label>
                    <textarea v-model="tool.outputsJson" />
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <div class="service-edit-modal-footer">
          <button class="btn ghost" @click="closeEditModal">取消</button>
          <button class="btn primary" @click="submitForm">保存</button>
        </div>
      </div>
    </div>

    <div
      v-if="showEmotionDimensionsModal"
      class="service-edit-modal-backdrop"
      @click.stop
    >
      <div class="service-edit-modal emotion-dim-modal" @click.stop>
        <div class="emotion-dim-modal-header">
          <h3 style="margin: 0">情绪维度</h3>
          <button
            class="emotion-dim-close-btn"
            @click="closeEmotionDimensionsModal"
          >
            ✕
          </button>
        </div>
        <div class="service-edit-modal-body">
          <div class="editor-card">
            <div class="split-header">
              <div>
                <h3>维度列表</h3>
              </div>
              <button
                class="btn ghost"
                type="button"
                :disabled="emotionDimensionAdding"
                @click="startAddEmotionDimension"
              >
                新增维度
              </button>
            </div>
            <div class="list" style="margin-top: 12px">
              <!-- 新增中的内联编辑卡片 -->
              <div
                v-if="emotionDimensionAdding"
                class="emotion-dim-card emotion-dim-card--editing"
              >
                <div class="emotion-dim-card-header">
                  <strong>新维度</strong>
                  <button
                    class="btn ghost connection-card-compact-btn"
                    type="button"
                    @click="cancelAddEmotionDimension"
                  >
                    取消
                  </button>
                </div>
                <div class="form-grid" style="margin-top: 10px">
                  <div class="field">
                    <label>名称</label>
                    <input
                      v-model="emotionDimensionDraft.name"
                      placeholder="例如：开心"
                    />
                  </div>
                  <div class="field">
                    <label>升权重 (0–20)</label>
                    <input
                      v-model.number="emotionDimensionDraft.increase_weight"
                      type="range"
                      min="0"
                      max="20"
                      step="0.1"
                    />
                    <span class="emotion-dim-weight-value">{{
                      emotionDimensionDraft.increase_weight
                    }}</span>
                  </div>
                  <div class="field">
                    <label>降权重 (0–20)</label>
                    <input
                      v-model.number="emotionDimensionDraft.decrease_weight"
                      type="range"
                      min="0"
                      max="20"
                      step="0.1"
                    />
                    <span class="emotion-dim-weight-value">{{
                      emotionDimensionDraft.decrease_weight
                    }}</span>
                  </div>
                  <div class="field-full">
                    <label>正向风格提示词（可选）</label>
                    <input
                      v-model="emotionDimensionDraft.positive_prompt"
                      placeholder="维度值正向时的语言风格，留空用维度名称"
                    />
                  </div>
                  <div class="field-full">
                    <label>负向风格提示词（可选）</label>
                    <input
                      v-model="emotionDimensionDraft.negative_prompt"
                      placeholder="维度值负向时的语言风格，留空用「不+维度名称」"
                    />
                  </div>
                </div>
                <div class="emotion-dim-card-actions">
                  <button
                    class="btn ghost"
                    type="button"
                    @click="cancelAddEmotionDimension"
                  >
                    取消
                  </button>
                  <button
                    class="btn primary"
                    type="button"
                    @click="confirmAddEmotionDimension"
                  >
                    新增
                  </button>
                </div>
              </div>

              <div
                v-if="
                  !emotionDimensionAdding &&
                  form.emotion_dimensions.length === 0
                "
                class="empty-state"
              >
                还没有配置情绪维度。点击「新增维度」开始添加。
              </div>

              <!-- 已有维度卡片 -->
              <div
                v-for="(dimension, index) in form.emotion_dimensions"
                :key="`${dimension.name}-${index}`"
                class="emotion-dim-card"
                :class="{ 'emotion-dim-card--editing': emotionDimensionEditingIndex === index }"
              >
                <!-- 编辑态 -->
                <template v-if="emotionDimensionEditingIndex === index">
                  <div class="emotion-dim-card-header">
                    <strong>编辑维度</strong>
                    <button
                      class="btn ghost connection-card-compact-btn"
                      type="button"
                      @click="cancelEditEmotionDimension"
                    >
                      取消
                    </button>
                  </div>
                  <div class="form-grid" style="margin-top: 10px">
                    <div class="field">
                      <label>名称</label>
                      <input
                        v-model="emotionDimensionDraft.name"
                        placeholder="例如：开心"
                      />
                    </div>
                    <div class="field">
                      <label>升权重 (0–20)</label>
                      <input
                        v-model.number="emotionDimensionDraft.increase_weight"
                        type="range"
                        min="0"
                        max="20"
                        step="0.1"
                      />
                      <span class="emotion-dim-weight-value">{{
                        emotionDimensionDraft.increase_weight
                      }}</span>
                    </div>
                    <div class="field">
                      <label>降权重 (0–20)</label>
                      <input
                        v-model.number="emotionDimensionDraft.decrease_weight"
                        type="range"
                        min="0"
                        max="20"
                        step="0.1"
                      />
                      <span class="emotion-dim-weight-value">{{
                        emotionDimensionDraft.decrease_weight
                      }}</span>
                    </div>
                    <div class="field-full">
                      <label>正向风格提示词（可选）</label>
                      <input
                        v-model="emotionDimensionDraft.positive_prompt"
                        placeholder="维度值正向时的语言风格提示，留空用维度名称"
                      />
                    </div>
                    <div class="field-full">
                      <label>负向风格提示词（可选）</label>
                      <input
                        v-model="emotionDimensionDraft.negative_prompt"
                        placeholder="维度值负向时的语言风格提示，留空用「不+维度名称」"
                      />
                    </div>
                  </div>
                  <div class="emotion-dim-card-actions">
                    <button
                      class="btn ghost"
                      type="button"
                      @click="cancelEditEmotionDimension"
                    >
                      取消
                    </button>
                    <button
                      class="btn primary"
                      type="button"
                      @click="confirmEditEmotionDimension"
                    >
                      保存
                    </button>
                  </div>
                </template>

                <!-- 展示态 -->
                <template v-else>
                  <div class="emotion-dim-card-header">
                    <strong>{{ dimension.name }}</strong>
                    <div class="inline-actions">
                      <button
                        class="btn ghost connection-card-compact-btn"
                        type="button"
                        :disabled="emotionDimensionAdding || emotionDimensionEditingIndex != null"
                        @click="editEmotionDimension(index)"
                      >
                        编辑
                      </button>
                      <button
                        class="btn warn connection-card-compact-btn"
                        type="button"
                        :disabled="emotionDimensionAdding || emotionDimensionEditingIndex != null"
                        @click="removeEmotionDimension(index)"
                      >
                        删除
                      </button>
                    </div>
                  </div>
                  <div class="emotion-dim-bars">
                    <div class="emotion-dim-bar-row">
                      <span class="emotion-dim-bar-label">升权重</span>
                      <div class="emotion-dim-bar-track">
                        <div
                          class="emotion-dim-bar-fill emotion-dim-bar-fill--increase"
                          :style="{
                            width:
                              Math.min(
                                ((dimension.increase_weight ?? 1) / 20) * 100,
                                100,
                              ) + '%',
                          }"
                        />
                      </div>
                      <span class="emotion-dim-bar-value">{{
                        dimension.increase_weight ?? 1
                      }}</span>
                    </div>
                    <div class="emotion-dim-bar-row">
                      <span class="emotion-dim-bar-label">降权重</span>
                      <div class="emotion-dim-bar-track">
                        <div
                          class="emotion-dim-bar-fill emotion-dim-bar-fill--decrease"
                          :style="{
                            width:
                              Math.min(
                                ((dimension.decrease_weight ?? 1) / 20) * 100,
                                100,
                              ) + '%',
                          }"
                        />
                      </div>
                      <span class="emotion-dim-bar-value">{{
                        dimension.decrease_weight ?? 1
                      }}</span>
                    </div>
                  </div>
                  <div v-if="dimension.positive_prompt || dimension.negative_prompt" class="emotion-dim-prompts" style="margin-top: 8px">
                    <div v-if="dimension.positive_prompt" class="emotion-dim-prompt-line">
                      <span class="emotion-dim-prompt-label">正向</span>
                      <span class="emotion-dim-prompt-text">{{ dimension.positive_prompt }}</span>
                    </div>
                    <div v-if="dimension.negative_prompt" class="emotion-dim-prompt-line">
                      <span class="emotion-dim-prompt-label">负向</span>
                      <span class="emotion-dim-prompt-text">{{ dimension.negative_prompt }}</span>
                    </div>
                  </div>
                </template>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div
      v-if="showIgnoreRulesModal"
      class="service-edit-modal-backdrop"
      @click.stop
    >
      <div class="service-edit-modal" @click.stop style="max-width: 760px">
        <div class="service-edit-modal-header">
          <h3 style="margin: 0">Ignore Rules</h3>
          <button class="btn ghost" @click="closeIgnoreRulesModal">关闭</button>
        </div>
        <div class="service-edit-modal-body">
          <div class="editor-card">
            <div class="split-header">
              <div>
                <h3>
                  {{ ignoreRuleForm.id == null ? "新增规则" : "编辑规则" }}
                </h3>
              </div>
            </div>
            <div class="form-grid" style="margin-top: 12px">
              <div class="field">
                <label>sender_id</label>
                <input
                  v-model="ignoreRuleForm.sender_id"
                  :disabled="ignoreRuleSubmitting"
                  placeholder="可空"
                />
              </div>
              <div class="field">
                <label>group_id</label>
                <input
                  v-model="ignoreRuleForm.group_id"
                  :disabled="ignoreRuleSubmitting"
                  placeholder="可空"
                />
              </div>
              <div class="field-full">
                <label>规则说明</label>
                <div class="muted">{{ ignoreRulePreview }}</div>
              </div>
            </div>
            <div
              v-if="ignoreRuleError"
              class="muted"
              style="margin-top: 12px; color: var(--danger, #d9534f)"
            >
              {{ ignoreRuleError }}
            </div>
            <div class="panel-actions" style="margin-top: 12px">
              <button
                class="btn ghost"
                :disabled="ignoreRuleSubmitting"
                @click="resetIgnoreRuleForm"
              >
                清空
              </button>
              <button
                class="btn primary"
                :disabled="ignoreRuleSubmitting"
                @click="submitIgnoreRule"
              >
                {{
                  ignoreRuleSubmitting
                    ? ignoreRuleForm.id == null
                      ? "新增中…"
                      : "保存中…"
                    : ignoreRuleForm.id == null
                      ? "新增"
                      : "保存"
                }}
              </button>
            </div>
          </div>

          <div class="editor-card" style="margin-top: 16px">
            <div class="split-header">
              <div>
                <h3>现有规则</h3>
              </div>
            </div>
            <div class="list" style="margin-top: 12px">
              <div v-if="ignoreRulesLoading" class="empty-state">加载中...</div>
              <div v-else-if="ignoreRules.length === 0" class="empty-state">
                还没有规则。
              </div>
              <div
                v-for="rule in ignoreRules"
                :key="rule.id"
                class="tool-block"
              >
                <div class="split-header">
                  <strong>#{{ rule.id }}</strong>
                  <div class="inline-actions">
                    <button
                      class="btn ghost connection-card-compact-btn"
                      :disabled="
                        ignoreRuleSubmitting || ignoreRuleDeletingId === rule.id
                      "
                      @click="editIgnoreRule(rule)"
                    >
                      编辑
                    </button>
                    <button
                      class="btn warn connection-card-compact-btn"
                      :disabled="
                        ignoreRuleSubmitting || ignoreRuleDeletingId === rule.id
                      "
                      @click="removeIgnoreRule(rule.id)"
                    >
                      {{
                        ignoreRuleDeletingId === rule.id ? "删除中…" : "删除"
                      }}
                    </button>
                  </div>
                </div>
                <div class="key-value">
                  <strong>sender_id</strong
                  ><span>{{ rule.sender_id || "未设置" }}</span>
                </div>
                <div class="key-value">
                  <strong>group_id</strong
                  ><span>{{ rule.group_id || "未设置" }}</span>
                </div>
                <div class="key-value">
                  <strong>含义</strong
                  ><span>{{
                    formatIgnoreRule(rule.sender_id, rule.group_id)
                  }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div
      v-if="showRateLimitModal"
      class="service-edit-modal-backdrop"
      @click.stop
    >
      <div class="service-edit-modal" @click.stop style="max-width: 820px">
        <div class="service-edit-modal-header">
          <h3 style="margin: 0">Rate Limit</h3>
          <button class="btn ghost" @click="closeRateLimitModal">关闭</button>
        </div>
        <div class="service-edit-modal-body">
          <div class="muted">
            调用频率限制，优先级：用户 &gt; 群组 &gt; 默认。窗口可按分钟 / 小时 / 天。
          </div>

          <div class="editor-card" style="margin-top: 12px">
            <div class="split-header">
              <div><h3>默认规则</h3></div>
              <label class="field-check">
                <input
                  v-model="form.message_rate_limit_default_enabled"
                  type="checkbox"
                />启用
              </label>
            </div>
            <div
              v-if="form.message_rate_limit_default_enabled"
              class="form-grid"
              style="margin-top: 12px"
            >
              <div class="field">
                <label>模式</label>
                <select v-model="form.message_rate_limit_default.unlimited">
                  <option :value="false">限次</option>
                  <option :value="true">无限</option>
                </select>
              </div>
              <template v-if="!form.message_rate_limit_default.unlimited">
                <div class="field">
                  <label>窗口</label>
                  <select v-model="form.message_rate_limit_default.window_unit">
                    <option value="minute">分钟</option>
                    <option value="hour">小时</option>
                    <option value="day">天</option>
                  </select>
                </div>
                <div class="field">
                  <label>次数</label>
                  <input
                    v-model.number="form.message_rate_limit_default.max_calls"
                    type="number"
                    min="1"
                  />
                </div>
              </template>
            </div>
          </div>

          <div class="editor-card" style="margin-top: 12px">
            <div class="split-header">
              <div><h3>群组规则</h3></div>
              <button class="btn ghost" type="button" @click="addGroupRateLimitRule">
                新增群组规则
              </button>
            </div>
            <div
              v-if="form.message_rate_limit_groups.length === 0"
              class="empty-state"
              style="margin-top: 12px"
            >
              还没有群组规则。
            </div>
            <div
              v-for="(rule, index) in form.message_rate_limit_groups"
              :key="`group-${index}`"
              class="tool-block"
              style="margin-top: 12px"
            >
              <div class="split-header">
                <strong>群组规则 {{ index + 1 }}</strong>
                <button
                  class="btn warn"
                  type="button"
                  @click="removeGroupRateLimitRule(index)"
                >
                  移除
                </button>
              </div>
              <div class="form-grid" style="margin-top: 12px">
                <div class="field">
                  <label>Group ID</label>
                  <input v-model="rule.group_id" />
                </div>
                <div class="field">
                  <label>模式</label>
                  <select v-model="rule.unlimited">
                    <option :value="false">限次</option>
                    <option :value="true">无限</option>
                  </select>
                </div>
                <template v-if="!rule.unlimited">
                  <div class="field">
                    <label>窗口</label>
                    <select v-model="rule.window_unit">
                      <option value="minute">分钟</option>
                      <option value="hour">小时</option>
                      <option value="day">天</option>
                    </select>
                  </div>
                  <div class="field">
                    <label>次数</label>
                    <input v-model.number="rule.max_calls" type="number" min="1" />
                  </div>
                </template>
              </div>
            </div>
          </div>

          <div class="editor-card" style="margin-top: 12px">
            <div class="split-header">
              <div><h3>用户规则</h3></div>
              <button class="btn ghost" type="button" @click="addUserRateLimitRule">
                新增用户规则
              </button>
            </div>
            <div
              v-if="form.message_rate_limit_users.length === 0"
              class="empty-state"
              style="margin-top: 12px"
            >
              还没有用户规则。
            </div>
            <div
              v-for="(rule, index) in form.message_rate_limit_users"
              :key="`user-${index}`"
              class="tool-block"
              style="margin-top: 12px"
            >
              <div class="split-header">
                <strong>用户规则 {{ index + 1 }}</strong>
                <button
                  class="btn warn"
                  type="button"
                  @click="removeUserRateLimitRule(index)"
                >
                  移除
                </button>
              </div>
              <div class="form-grid" style="margin-top: 12px">
                <div class="field">
                  <label>Sender ID</label>
                  <input v-model="rule.sender_id" />
                </div>
                <div class="field">
                  <label>模式</label>
                  <select v-model="rule.unlimited">
                    <option :value="false">限次</option>
                    <option :value="true">无限</option>
                  </select>
                </div>
                <template v-if="!rule.unlimited">
                  <div class="field">
                    <label>窗口</label>
                    <select v-model="rule.window_unit">
                      <option value="minute">分钟</option>
                      <option value="hour">小时</option>
                      <option value="day">天</option>
                    </select>
                  </div>
                  <div class="field">
                    <label>次数</label>
                    <input v-model.number="rule.max_calls" type="number" min="1" />
                  </div>
                </template>
              </div>
            </div>
          </div>

          <div class="panel-actions" style="margin-top: 16px">
            <button class="btn primary" @click="closeRateLimitModal">完成</button>
          </div>
        </div>
      </div>
    </div>

    <section v-if="servicesLoading && services.length === 0" class="panel">
      <div class="service-loading-state" aria-live="polite">
        <span class="service-loading-spinner"></span>
        <span>Service 加载中...</span>
      </div>
    </section>

    <section v-else-if="services.length > 0" class="panel">
      <div
        class="connection-grid connection-grid--services"
        style="margin-top: 0"
      >
        <article
          v-for="service in services"
          :key="service.config_id"
          class="connection-card"
        >
          <div class="connection-card-header connection-card-header--stacked">
            <div class="connection-card-header-top">
              <div class="connection-card-badges">
                <span class="badge">{{ service.agent_type.type }}</span>
                <span class="badge" :class="service.enabled ? 'success' : ''">{{
                  service.enabled ? "已启用" : "已停用"
                }}</span>
                <span class="badge" :class="statusTone(service.runtime.status)">{{
                  runtimeBadgeText(service)
                }}</span>
                <span v-if="service.is_default" class="badge">default</span>
              </div>
              <div class="inline-actions connection-card-display-actions">
                <button
                  class="btn ghost connection-card-compact-btn"
                  @click="editService(service)"
                >
                  编辑
                </button>
                <button
                  class="btn connection-card-compact-btn"
                  @click="toggleServiceRuntime(service)"
                >
                  {{ service.runtime.status === "running" ? "停止" : "启动" }}
                </button>
                <button
                  class="btn warn connection-card-compact-btn"
                  @click="removeService(service.config_id)"
                >
                  删除
                </button>
              </div>
            </div>
            <div style="display: flex; align-items: center; gap: 10px">
              <img
                v-if="agentAvatarUrl(service)"
                :src="agentAvatarUrl(service)"
                alt="bot avatar"
                style="
                  width: 36px;
                  height: 36px;
                  border-radius: 999px;
                  border: 1px solid var(--line);
                  object-fit: cover;
                  background: var(--surface-soft);
                "
              />
              <h4 style="margin: 0">{{ service.name }}</h4>
            </div>
          </div>

          <div class="connection-card-body">
            <div
              v-for="item in summarizeService(service)"
              :key="item.label"
              class="key-value"
            >
              <strong>{{ item.label }}</strong>
              <span :class="item.mono ? 'mono' : ''">{{ item.value }}</span>
            </div>
          </div>

          <div class="connection-card-footer">
            <span class="muted"
              >启动于 {{ formatTime(service.runtime.started_at) }}</span
            >
            <span class="muted">工具 {{ service.tools.length }} 个</span>
          </div>
        </article>
      </div>
    </section>

    <section v-else class="panel">
      <div class="empty-state">当前没有 Service。</div>
    </section>
  </section>
</template>

<script setup lang="ts">
import { useAgents } from "../composables/useAgents";

const {
  serviceTypes,
  services,
  servicesLoading,
  connections,
  llm,
  workflows,
  form,
  editingServiceId,
  showCreatePicker,
  showCreateForm,
  showEditModal,
  showEmotionDimensionsModal,
  showRateLimitModal,
  showIgnoreRulesModal,
  ignoreRulesLoading,
  ignoreRules,
  ignoreRuleSubmitting,
  ignoreRuleDeletingId,
  ignoreRuleError,
  ignoreRuleForm,
  emotionDimensionAdding,
  emotionDimensionDraft,
  emotionDimensionEditingIndex,
  qqChatDefaultTools,
  httpStreamDefaultTools,
  workspaceDefaultTools,
  currentDefaultTools,
  defaultToolSearchQuery,
  filteredDefaultTools,
  showDefaultToolEditModal,
  editingDefaultToolId,
  defaultToolEditDraft,
  currentEditingDefaultTool,
  openDefaultToolEditModal,
  closeDefaultToolEditModal,
  confirmDefaultToolEdit,
  chatModels,
  multimodalChatModels,
  embeddingModels,
  mainChatModel,
  mainChatModelSupportsMultimodal,
  botConnections,
  rustfsConnections,
  webSearchEngineConnections,
  taskDbConnections,
  tokenizerConnections,
  imageWeaviateConnections,
  memoryWeaviateConnections,
  ignoreRulesDisabledReason,
  resetForm,
  avatarUploading,
  handleAvatarFileSelect,
  uploadAvatarFile,
  clearAvatar,
  clearEditingAgent,
  ignoreRulePreview,
  formatRequestError,
  startCreate,
  closeCreatePicker,
  pickCreateType,
  closeEditor,
  load,
  editService,
  closeEditModal,
  openEmotionDimensionsModal,
  closeEmotionDimensionsModal,
  resetEmotionDimensionDraft,
  startAddEmotionDimension,
  cancelAddEmotionDimension,
  buildEmotionDimensionPayload,
  confirmAddEmotionDimension,
  editEmotionDimension,
  cancelEditEmotionDimension,
  confirmEditEmotionDimension,
  removeEmotionDimension,
  resetIgnoreRuleForm,
  formatIgnoreRule,
  loadIgnoreRules,
  openIgnoreRulesModal,
  closeIgnoreRulesModal,
  openRateLimitModal,
  closeRateLimitModal,
  addGroupRateLimitRule,
  removeGroupRateLimitRule,
  addUserRateLimitRule,
  removeUserRateLimitRule,
  editIgnoreRule,
  submitIgnoreRule,
  removeIgnoreRule,
  addTool,
  removeTool,
  validateImageUnderstandModelSelection,
  isGeneratedToolId,
  syncingToolIndex,
  syncToolFromGraph,
  handleToolTargetTypeChange,
  applyWorkflowSetMetadata,
  submitForm,
  removeService,
  startAgent,
  stopAgent,
  toggleServiceRuntime,
  summarizeService,
  connectionName,
  llmName,
  llmRefName,
  runtimeBadgeText,
  compactId,
  formatTime,
  statusTone,
  summarizeIds,
  getAvatarDisplayUrl,
  agentAvatarUrl,
  agentInitial,
} = useAgents();
</script>

<style scoped lang="scss">
@use "../styles/agents" as *;
@use "../styles/connections" as *;
</style>
