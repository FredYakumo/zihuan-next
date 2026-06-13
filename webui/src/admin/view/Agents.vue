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
              <div class="field-full">
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

            <template v-if="form.type === 'workspace'">
              <div class="editor-card" style="margin-top: 12px">
                <div class="split-header">
                  <div>
                    <h3>默认工具</h3>
                  </div>
                </div>
                <div class="list" style="margin-top: 12px">
                  <label
                    v-for="tool in workspaceDefaultTools"
                    :key="tool.id"
                    class="field-check"
                    style="
                      display: flex;
                      align-items: flex-start;
                      gap: 8px;
                      margin-bottom: 8px;
                    "
                  >
                    <input
                      v-model="form.default_tools_enabled[tool.id]"
                      type="checkbox"
                    />
                    <span>
                      <strong>{{ tool.label }}</strong>
                      <span class="muted" style="display: block">{{
                        tool.description
                      }}</span>
                    </span>
                  </label>
                </div>
              </div>
            </template>
          </div>

          <div
            v-if="form.type === 'qq_chat'"
            class="editor-card"
            style="margin-top: 12px"
          >
            <div class="split-header">
              <div>
                <h3>默认工具</h3>
              </div>
            </div>
            <div class="list" style="margin-top: 12px">
              <label
                v-for="tool in qqChatDefaultTools"
                :key="tool.id"
                class="field-check"
                style="
                  display: flex;
                  align-items: flex-start;
                  gap: 8px;
                  margin-bottom: 8px;
                "
              >
                <input
                  v-model="form.default_tools_enabled[tool.id]"
                  type="checkbox"
                />
                <span style="flex: 1">
                  <strong>{{ tool.label }}</strong>
                  <span class="muted" style="display: block">{{
                    tool.description
                  }}</span>
                  <div
                    v-if="
                      tool.id === 'image_understand' &&
                      form.default_tools_enabled.image_understand !== false
                    "
                    style="margin-top: 8px"
                  >
                    <label>图片理解模型</label>
                    <select v-model="form.image_understand_llm_ref_id">
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
                      image_understand 默认使用 Service
                      主模型；这里只有支持多模态的模型可选。
                    </div>
                    <div
                      v-if="
                        form.llm_ref_id &&
                        !mainChatModelSupportsMultimodal &&
                        !form.image_understand_llm_ref_id
                      "
                      class="muted"
                      style="margin-top: 4px; color: #ffb36b"
                    >
                      当前主模型不支持多模态，启用 image_understand
                      时必须在这里指定一个支持多模态的模型。
                    </div>
                  </div>
                </span>
              </label>
            </div>
          </div>

          <div v-else-if="form.type === 'http_stream'" class="editor-card" style="margin-top: 12px">
            <div class="split-header">
              <div>
                <h3>默认工具</h3>
              </div>
            </div>
            <div class="list" style="margin-top: 12px">
              <label
                v-for="tool in httpStreamDefaultTools"
                :key="tool.id"
                class="field-check"
                style="
                  display: flex;
                  align-items: flex-start;
                  gap: 8px;
                  margin-bottom: 8px;
                "
              >
                <input
                  v-model="form.default_tools_enabled[tool.id]"
                  type="checkbox"
                />
                <span>
                  <strong>{{ tool.label }}</strong>
                  <span class="muted" style="display: block">{{
                    tool.description
                  }}</span>
                </span>
              </label>
            </div>
          </div>

<!-- 头像编辑：http_stream 和 workspace 支持 -->
            <template v-if="form.type === 'http_stream' || form.type === 'workspace'">
              <div class="field-full">
                <label>Service 头像</label>
                <div class="avatar-upload-row">
                  <img
                    v-if="form.avatar_url"
                    :src="form.avatar_url"
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

            <template v-if="form.type === 'workspace'">
            <div class="editor-card" style="margin-top: 12px">
              <div class="split-header">
                <div>
                  <h3>默认工具</h3>
                </div>
              </div>
              <div class="list" style="margin-top: 12px">
                <label
                  v-for="tool in workspaceDefaultTools"
                  :key="tool.id"
                  class="field-check"
                  style="
                    display: flex;
                    align-items: flex-start;
                    gap: 8px;
                    margin-bottom: 8px;
                  "
                >
                  <input
                    v-model="form.default_tools_enabled[tool.id]"
                    type="checkbox"
                  />
                  <span>
                    <strong>{{ tool.label }}</strong>
                    <span class="muted" style="display: block">{{
                      tool.description
                    }}</span>
                  </span>
                </label>
              </div>
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
              <div class="field-full">
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

              <div class="editor-card" style="margin-top: 12px">
                <div class="split-header">
                  <div>
                    <h3>默认工具</h3>
                  </div>
                </div>
                <div class="list" style="margin-top: 12px">
                  <label
                    v-for="tool in qqChatDefaultTools"
                    :key="tool.id"
                    class="field-check"
                    style="
                      display: flex;
                      align-items: flex-start;
                      gap: 8px;
                      margin-bottom: 8px;
                    "
                  >
                    <input
                      v-model="form.default_tools_enabled[tool.id]"
                      type="checkbox"
                    />
                    <span style="flex: 1">
                      <strong>{{ tool.label }}</strong>
                      <span class="muted" style="display: block">{{
                        tool.description
                      }}</span>
                      <div
                        v-if="
                          tool.id === 'image_understand' &&
                          form.default_tools_enabled.image_understand !== false
                        "
                        style="margin-top: 8px"
                      >
                        <label>图片理解模型</label>
                        <select v-model="form.image_understand_llm_ref_id">
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
                          image_understand 默认使用 Service
                          主模型；这里只有支持多模态的模型可选。
                        </div>
                        <div
                          v-if="
                            form.llm_ref_id &&
                            !mainChatModelSupportsMultimodal &&
                            !form.image_understand_llm_ref_id
                          "
                          class="muted"
                          style="margin-top: 4px; color: #ffb36b"
                        >
                          当前主模型不支持多模态，启用 image_understand
                          时必须在这里指定一个支持多模态的模型。
                        </div>
                      </div>
                    </span>
                  </label>
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

              <div class="editor-card" style="margin-top: 12px">
                <div class="split-header">
                  <div>
                    <h3>默认工具</h3>
                  </div>
                </div>
                <div class="list" style="margin-top: 12px">
                  <label
                    v-for="tool in httpStreamDefaultTools"
                    :key="tool.id"
                    class="field-check"
                    style="
                      display: flex;
                      align-items: flex-start;
                      gap: 8px;
                      margin-bottom: 8px;
                    "
                  >
                    <input
                      v-model="form.default_tools_enabled[tool.id]"
                      type="checkbox"
                    />
                    <span>
                      <strong>{{ tool.label }}</strong>
                      <span class="muted" style="display: block">{{
                        tool.description
                      }}</span>
                    </span>
                  </label>
                </div>
              </div>
            </template>

            <template v-if="form.type === 'workspace'">
              <div class="editor-card" style="margin-top: 12px">
                <div class="split-header">
                  <div>
                    <h3>默认工具</h3>
                  </div>
                </div>
                <div class="list" style="margin-top: 12px">
                  <label
                    v-for="tool in workspaceDefaultTools"
                    :key="tool.id"
                    class="field-check"
                    style="
                      display: flex;
                      align-items: flex-start;
                      gap: 8px;
                      margin-bottom: 8px;
                    "
                  >
                    <input
                      v-model="form.default_tools_enabled[tool.id]"
                      type="checkbox"
                    />
                    <span>
                      <strong>{{ tool.label }}</strong>
                      <span class="muted" style="display: block">{{
                        tool.description
                      }}</span>
                    </span>
                  </label>
                </div>
              </div>
            </template>
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
                v-if="botAvatarUrl(service)"
                :src="botAvatarUrl(service)"
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
import { computed, onMounted, reactive, ref } from "vue";

import {
  system,
  workflows as workflowApi,
  type ServiceWithRuntime,
  type ConnectionConfig,
  type LlmConfig,
  type QqChatAgentServiceIgnoreRule,
  type WorkflowInfo,
} from "../../api/client";
import {
  serviceFormFromConfig,
  buildServicePayload,
  HTTP_STREAM_DEFAULT_TOOLS,
  WORKSPACE_DEFAULT_TOOLS,
  isBotAdapterConnectionType,
  QQ_CHAT_DEFAULT_TOOLS,
  defaultServiceForm,
  defaultHttpStreamDefaultToolsEnabled,
  defaultQqChatDefaultToolsEnabled,
  defaultToolForm,
  defaultWorkspaceDefaultToolsEnabled,
  compactId,
  formatTime,
  statusTone,
  summarizeIds,
  type ServiceFormState,
  type ServiceTypeName,
  type QqChatEmotionDimensionFormItem,
} from "../model";

type ServiceTypeOption = {
  value: ServiceTypeName;
  label: string;
  hint: string;
};

const serviceTypes: ServiceTypeOption[] = [
  {
    value: "qq_chat",
    label: "QQ Chat Agent Service",
    hint: "通过 QQ Bot Adapter 提供对话服务",
  },
  {
    value: "http_stream",
    label: "HTTP Stream Service",
    hint: "通过 HTTP 流式接口对外提供服务",
  },
  {
    value: "workspace",
    label: "Workspace Agent Service",
    hint: "面向项目目录的开发型 Agent Service",
  },
];

const services = ref<ServiceWithRuntime[]>([]);
const servicesLoading = ref(false);
const connections = ref<ConnectionConfig[]>([]);
const llm = ref<LlmConfig[]>([]);
const workflows = ref<WorkflowInfo[]>([]);
const form = reactive<ServiceFormState>(defaultServiceForm());
const editingServiceId = ref("");
const showCreatePicker = ref(false);
const showCreateForm = ref(false);
const showEditModal = ref(false);
const showEmotionDimensionsModal = ref(false);
const showIgnoreRulesModal = ref(false);
const ignoreRulesLoading = ref(false);
const ignoreRules = ref<QqChatAgentServiceIgnoreRule[]>([]);
const ignoreRuleSubmitting = ref(false);
const ignoreRuleDeletingId = ref<number | null>(null);
const ignoreRuleError = ref("");
const ignoreRuleForm = reactive<{
  id: number | null;
  sender_id: string;
  group_id: string;
}>({
  id: null,
  sender_id: "",
  group_id: "",
});
const emotionDimensionAdding = ref(false);
const emotionDimensionDraft = reactive<{
  name: string;
  increase_weight: number;
  decrease_weight: number;
  positive_prompt: string;
  negative_prompt: string;
}>({
  name: "",
  increase_weight: 1,
  decrease_weight: 1,
  positive_prompt: "",
  negative_prompt: "",
});
const emotionDimensionEditingIndex = ref<number | null>(null);
const qqChatDefaultTools = QQ_CHAT_DEFAULT_TOOLS;
const httpStreamDefaultTools = HTTP_STREAM_DEFAULT_TOOLS;
const workspaceDefaultTools = WORKSPACE_DEFAULT_TOOLS;
const chatModels = computed(() =>
  llm.value.filter((item) => item.model.type === "chat_llm"),
);
const multimodalChatModels = computed(() =>
  llm.value.filter(
    (item) =>
      item.model.type === "chat_llm" &&
      Boolean(item.model.llm.supports_multimodal_input),
  ),
);
const embeddingModels = computed(() =>
  llm.value.filter(
    (item) => item.model.type === "text_embedding_local" && item.enabled,
  ),
);
const mainChatModel = computed(() =>
  llm.value.find((item) => item.config_id === form.llm_ref_id),
);
const mainChatModelSupportsMultimodal = computed(() => {
  const selected = mainChatModel.value;
  return Boolean(
    selected?.model.type === "chat_llm" &&
    selected.model.llm.supports_multimodal_input,
  );
});

const botConnections = computed(() =>
  connections.value.filter((item) =>
    isBotAdapterConnectionType(String(item.kind.type ?? "")),
  ),
);
const rustfsConnections = computed(() =>
  connections.value.filter((item) => item.kind.type === "rustfs"),
);
const webSearchEngineConnections = computed(() =>
  connections.value.filter((item) => item.kind.type === "web_search_engine"),
);
const taskDbConnections = computed(() =>
  connections.value.filter(
    (item) => item.kind.type === "mysql" || item.kind.type === "sqlite",
  ),
);
const tokenizerConnections = computed(() =>
  connections.value.filter((item) => item.kind.type === "tokenizer"),
);
const imageWeaviateConnections = computed(() =>
  connections.value.filter(
    (item) =>
      item.kind.type === "weaviate" &&
      item.kind.collection_schema === "image_semantic",
  ),
);
const memoryWeaviateConnections = computed(() =>
  connections.value.filter(
    (item) =>
      item.kind.type === "weaviate" &&
      item.kind.collection_schema === "agent_memory",
  ),
);
const ignoreRulesDisabledReason = computed(() => {
  if (!editingServiceId.value) {
    return "请先保存当前 Service，再管理 Ignore Rules。";
  }
  if (!form.rdb_id) {
    return "先配置 RDB Connection，Ignore Rules 和任务/消息持久化都会共用这条关系库连接。";
  }
  return "";
});

function resetForm() {
  Object.assign(form, defaultServiceForm());
  emotionDimensionAdding.value = false;
  emotionDimensionEditingIndex.value = null;
  resetEmotionDimensionDraft();
}

const avatarUploading = ref(false);

function handleAvatarFileSelect(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;

  // Validate file type
  if (!file.type.startsWith('image/')) {
    alert('请上传图片文件');
    return;
  }

  // Validate file size (max 30MB)
  const maxSize = 30 * 1024 * 1024;
  if (file.size > maxSize) {
    alert('图片大小不能超过 30MB');
    return;
  }

  uploadAvatarFile(file);

  // Reset input
  input.value = '';
}

async function uploadAvatarFile(file: File) {
  if (avatarUploading.value) return;

  avatarUploading.value = true;
  try {
    const formData = new FormData();
    formData.append('file', file);

    const response = await fetch('/api/system/services/avatar', {
      method: 'POST',
      body: formData,
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(error || '上传失败');
    }

    const result = await response.json();
    if (result.avatar_id) {
      // Store avatar:// prefix to distinguish from external URLs
      form.avatar_url = `avatar://${result.avatar_id}`;
    }
  } catch (e) {
    alert(`头像上传失败: ${e}`);
  } finally {
    avatarUploading.value = false;
  }
}

function clearAvatar() {
  form.avatar_url = '';
}

// Get display URL for avatar (handles avatar:// prefix)
function getAvatarDisplayUrl(avatarUrl: string): string {
  if (!avatarUrl) return '';
  if (avatarUrl.startsWith('avatar://')) {
    const avatarId = avatarUrl.substring(9);
    return `/api/system/services/avatar/${avatarId}`;
  }
  // External URL or base64
  return avatarUrl;
}

function clearEditingAgent() {
  editingServiceId.value = "";
}

const ignoreRulePreview = computed(() =>
  formatIgnoreRule(ignoreRuleForm.sender_id, ignoreRuleForm.group_id),
);

function formatRequestError(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  return "请求失败，请稍后重试";
}

function startCreate() {
  resetForm();
  clearEditingAgent();
  showCreatePicker.value = true;
  showCreateForm.value = false;
}

function closeCreatePicker() {
  resetForm();
  clearEditingAgent();
  showEmotionDimensionsModal.value = false;
  showCreatePicker.value = false;
  showCreateForm.value = false;
}

function pickCreateType(type: ServiceTypeName) {
  resetForm();
  clearEditingAgent();
  form.type = type;
  if (type === "qq_chat") {
    form.default_tools_enabled = defaultQqChatDefaultToolsEnabled();
  } else if (type === "http_stream") {
    form.default_tools_enabled = defaultHttpStreamDefaultToolsEnabled();
  } else {
    form.default_tools_enabled = defaultWorkspaceDefaultToolsEnabled();
  }
  showCreatePicker.value = true;
  showCreateForm.value = true;
}

function closeEditor() {
  showCreatePicker.value = false;
  showCreateForm.value = false;
  closeEditModal();
}

async function load() {
  servicesLoading.value = true;
  try {
    const [loadedAgents, loadedConnections, loadedLlm, loadedWorkflows] =
      await Promise.all([
        system.services.list(),
        system.connections.list(),
        system.llm.list(),
        workflowApi.listDetailed(),
      ]);
    services.value = loadedAgents;
    connections.value = loadedConnections;
    llm.value = loadedLlm;
    workflows.value = loadedWorkflows.workflows;
  } finally {
    servicesLoading.value = false;
  }
}

function editService(service: ServiceWithRuntime) {
  Object.assign(form, serviceFormFromConfig(service));
  editingServiceId.value = service.config_id;
  showEditModal.value = true;
}

function closeEditModal() {
  showEmotionDimensionsModal.value = false;
  showEditModal.value = false;
  resetForm();
  clearEditingAgent();
}
function openEmotionDimensionsModal() {
  resetEmotionDimensionDraft();
  emotionDimensionAdding.value = false;
  emotionDimensionEditingIndex.value = null;
  showEmotionDimensionsModal.value = true;
}

function closeEmotionDimensionsModal() {
  showEmotionDimensionsModal.value = false;
  emotionDimensionAdding.value = false;
  emotionDimensionEditingIndex.value = null;
  resetEmotionDimensionDraft();
}

function resetEmotionDimensionDraft() {
  emotionDimensionDraft.name = "";
  emotionDimensionDraft.increase_weight = 1;
  emotionDimensionDraft.decrease_weight = 1;
  emotionDimensionDraft.positive_prompt = "";
  emotionDimensionDraft.negative_prompt = "";
}

function startAddEmotionDimension() {
  resetEmotionDimensionDraft();
  emotionDimensionEditingIndex.value = null;
  emotionDimensionAdding.value = true;
}

function cancelAddEmotionDimension() {
  emotionDimensionAdding.value = false;
  resetEmotionDimensionDraft();
}

function buildEmotionDimensionPayload(): QqChatEmotionDimensionFormItem | null {
  const name = emotionDimensionDraft.name.trim();
  if (!name) {
    alert("请填写情绪维度名称");
    return null;
  }
  if (
    !Number.isFinite(emotionDimensionDraft.increase_weight) ||
    emotionDimensionDraft.increase_weight < 0
  ) {
    alert("升权重不能为负数");
    return null;
  }
  if (
    !Number.isFinite(emotionDimensionDraft.decrease_weight) ||
    emotionDimensionDraft.decrease_weight < 0
  ) {
    alert("降权重不能为负数");
    return null;
  }
  return {
    name,
    increase_weight: emotionDimensionDraft.increase_weight,
    decrease_weight: emotionDimensionDraft.decrease_weight,
    positive_prompt: emotionDimensionDraft.positive_prompt.trim() || undefined,
    negative_prompt: emotionDimensionDraft.negative_prompt.trim() || undefined,
  };
}

function confirmAddEmotionDimension() {
  const payload = buildEmotionDimensionPayload();
  if (!payload) {
    return;
  }

  const duplicateIndex = form.emotion_dimensions.findIndex(
    (item) => item.name.trim() === payload.name,
  );
  if (duplicateIndex >= 0) {
    alert(`情绪维度 '${payload.name}' 已存在`);
    return;
  }

  form.emotion_dimensions.unshift(payload);
  emotionDimensionAdding.value = false;
  resetEmotionDimensionDraft();
}

function editEmotionDimension(index: number) {
  const dimension = form.emotion_dimensions[index];
  if (!dimension) {
    return;
  }
  emotionDimensionAdding.value = false;
  emotionDimensionEditingIndex.value = index;
  emotionDimensionDraft.name = dimension.name;
  emotionDimensionDraft.increase_weight = Number(dimension.increase_weight ?? 1);
  emotionDimensionDraft.decrease_weight = Number(dimension.decrease_weight ?? 1);
  emotionDimensionDraft.positive_prompt = dimension.positive_prompt ?? "";
  emotionDimensionDraft.negative_prompt = dimension.negative_prompt ?? "";
}

function cancelEditEmotionDimension() {
  emotionDimensionEditingIndex.value = null;
  resetEmotionDimensionDraft();
}

function confirmEditEmotionDimension() {
  if (emotionDimensionEditingIndex.value == null) {
    return;
  }
  const payload = buildEmotionDimensionPayload();
  if (!payload) {
    return;
  }

  const duplicateIndex = form.emotion_dimensions.findIndex(
    (item, index) =>
      item.name.trim() === payload.name &&
      index !== emotionDimensionEditingIndex.value,
  );
  if (duplicateIndex >= 0) {
    alert(`情绪维度 '${payload.name}' 已存在`);
    return;
  }

  form.emotion_dimensions.splice(emotionDimensionEditingIndex.value, 1, payload);
  emotionDimensionEditingIndex.value = null;
  resetEmotionDimensionDraft();
}

function removeEmotionDimension(index: number) {
  const dimension = form.emotion_dimensions[index];
  if (!dimension) {
    return;
  }
  if (!window.confirm(`确认删除情绪维度 '${dimension.name}' 吗？`)) {
    return;
  }
  form.emotion_dimensions.splice(index, 1);
  if (emotionDimensionEditingIndex.value === index) {
    emotionDimensionEditingIndex.value = null;
    resetEmotionDimensionDraft();
    return;
  }
  if (
    emotionDimensionEditingIndex.value != null &&
    emotionDimensionEditingIndex.value > index
  ) {
    emotionDimensionEditingIndex.value -= 1;
  }
}

function resetIgnoreRuleForm() {
  ignoreRuleForm.id = null;
  ignoreRuleForm.sender_id = "";
  ignoreRuleForm.group_id = "";
  ignoreRuleError.value = "";
}

function formatIgnoreRule(
  senderId: string | null | undefined,
  groupId: string | null | undefined,
): string {
  const sender = String(senderId ?? "").trim();
  const group = String(groupId ?? "").trim();
  if (sender && group) {
    return `屏蔽群 ${group} 下的 QQ ${sender}`;
  }
  if (sender) {
    return `屏蔽 QQ ${sender}`;
  }
  if (group) {
    return `屏蔽群 ${group}`;
  }
  return "至少填写 sender_id 或 group_id 其中一个";
}

async function loadIgnoreRules() {
  if (!editingServiceId.value) {
    return;
  }
  ignoreRulesLoading.value = true;
  try {
    ignoreRuleError.value = "";
    ignoreRules.value = await system.services.listIgnoreRules(
      editingServiceId.value,
    );
  } catch (error) {
    ignoreRuleError.value = `加载 Ignore Rules 失败: ${formatRequestError(error)}`;
  } finally {
    ignoreRulesLoading.value = false;
  }
}

async function openIgnoreRulesModal() {
  if (ignoreRulesDisabledReason.value) {
    alert(ignoreRulesDisabledReason.value);
    return;
  }
  resetIgnoreRuleForm();
  showIgnoreRulesModal.value = true;
  await loadIgnoreRules();
}

function closeIgnoreRulesModal() {
  showIgnoreRulesModal.value = false;
  resetIgnoreRuleForm();
  ignoreRuleDeletingId.value = null;
}

function editIgnoreRule(rule: QqChatAgentServiceIgnoreRule) {
  ignoreRuleForm.id = rule.id;
  ignoreRuleForm.sender_id = rule.sender_id ?? "";
  ignoreRuleForm.group_id = rule.group_id ?? "";
}

async function submitIgnoreRule() {
  if (!editingServiceId.value) {
    return;
  }
  const payload = {
    sender_id: ignoreRuleForm.sender_id.trim() || null,
    group_id: ignoreRuleForm.group_id.trim() || null,
  };
  if (!payload.sender_id && !payload.group_id) {
    alert("sender_id 和 group_id 至少填写一个");
    return;
  }
  ignoreRuleSubmitting.value = true;
  ignoreRuleError.value = "";
  try {
    if (ignoreRuleForm.id == null) {
      await system.services.createIgnoreRule(editingServiceId.value, payload);
    } else {
      await system.services.updateIgnoreRule(
        editingServiceId.value,
        ignoreRuleForm.id,
        payload,
      );
    }
    resetIgnoreRuleForm();
    await loadIgnoreRules();
  } catch (error) {
    ignoreRuleError.value = `保存 Ignore Rule 失败: ${formatRequestError(error)}`;
  } finally {
    ignoreRuleSubmitting.value = false;
  }
}

async function removeIgnoreRule(ruleId: number) {
  if (!editingServiceId.value) {
    return;
  }
  if (!window.confirm("确认删除这条 Ignore Rule 吗？")) {
    return;
  }
  ignoreRuleDeletingId.value = ruleId;
  ignoreRuleError.value = "";
  try {
    await system.services.deleteIgnoreRule(editingServiceId.value, ruleId);
    if (ignoreRuleForm.id === ruleId) {
      resetIgnoreRuleForm();
    }
    await loadIgnoreRules();
  } catch (error) {
    ignoreRuleError.value = `删除 Ignore Rule 失败: ${formatRequestError(error)}`;
  } finally {
    ignoreRuleDeletingId.value = null;
  }
}

function addTool() {
  form.tools.push(defaultToolForm());
}

function removeTool(index: number) {
  form.tools.splice(index, 1);
}

function validateImageUnderstandModelSelection(): string | null {
  if (form.type !== "qq_chat" || !form.default_tools_enabled.image_understand) {
    return null;
  }
  if (form.image_understand_llm_ref_id) {
    const selected = llm.value.find(
      (item) => item.config_id === form.image_understand_llm_ref_id,
    );
    if (
      !selected ||
      selected.model.type !== "chat_llm" ||
      !selected.model.llm.supports_multimodal_input
    ) {
      return "image_understand 需要选择一个支持多模态的模型";
    }
    return null;
  }
  if (!mainChatModelSupportsMultimodal.value) {
    return "image_understand 已启用时，主模型不支持多模态，请选择一个支持多模态的模型";
  }
  return null;
}

const RESERVED_TOOL_RUNTIME_INPUTS = new Set([
  "content",
  "message_event",
  "qq_ims_bot_adapter",
]);

function isGeneratedToolId(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
    value.trim(),
  );
}

const syncingToolIndex = ref<number | null>(null);

async function syncToolFromGraph(
  tool: ServiceFormState["tools"][number],
  index: number,
) {
  syncingToolIndex.value = index;
  try {
    const result = await workflowApi.listDetailed();
    workflows.value = result.workflows;
    applyWorkflowSetMetadata(tool);
  } finally {
    syncingToolIndex.value = null;
  }
}

function handleToolTargetTypeChange(tool: ServiceFormState["tools"][number]) {
  if (tool.targetType === "workflow_set" && tool.workflowName) {
    applyWorkflowSetMetadata(tool);
  }
}

function applyWorkflowSetMetadata(tool: ServiceFormState["tools"][number]) {
  if (tool.targetType !== "workflow_set" || !tool.workflowName) {
    return;
  }
  const workflow = workflows.value.find(
    (item) => item.name === tool.workflowName,
  );
  if (!workflow) {
    return;
  }

  if (!tool.id.trim() || isGeneratedToolId(tool.id)) {
    tool.id = workflow.name;
  }
  tool.name = workflow.name;
  tool.description = workflow.description ?? "";
  tool.parametersJson = JSON.stringify(
    (workflow.inputs ?? [])
      .filter((port) => !RESERVED_TOOL_RUNTIME_INPUTS.has(port.name))
      .map((port) => ({
        name: port.name,
        data_type: port.data_type,
        desc: port.description ?? "",
      })),
    null,
    2,
  );
  tool.outputsJson = JSON.stringify(
    (workflow.outputs ?? []).map((port) => ({
      name: port.name,
      data_type: port.data_type,
      desc: port.description ?? "",
    })),
    null,
    2,
  );
}

async function submitForm() {
  try {
    const payload = buildServicePayload(form);
    if (!payload.name) {
      alert("请填写 Agent 名称");
      return;
    }
    if (!form.llm_ref_id) {
      alert("请绑定一个模型配置");
      return;
    }
    if (form.type === "qq_chat" && !form.ims_bot_adapter_connection_id) {
      alert("QQ Chat Agent Service 需要绑定 Bot Adapter");
      return;
    }
    if (form.type === "qq_chat" && !form.web_search_engine_connection_id) {
      alert("QQ Chat Agent Service 需要绑定 Web Search Engine 连接");
      return;
    }
    const imageUnderstandError = validateImageUnderstandModelSelection();
    if (imageUnderstandError) {
      alert(imageUnderstandError);
      return;
    }
    if (
      form.type === "http_stream" &&
      form.default_tools_enabled.web_search !== false &&
      !form.http_web_search_engine_connection_id
    ) {
      alert(
        "启用 web_search 时，HTTP stream service 需要绑定 Web Search Engine 连接",
      );
      return;
    }
    if (
      form.type === "qq_chat" &&
      form.weaviate_memory_connection_id &&
      !form.embedding_model_ref_id
    ) {
      alert("QQ Chat Agent Service 启用记忆库时需要绑定文本向量模型");
      return;
    }
    if (
      form.type === "http_stream" &&
      form.http_weaviate_memory_connection_id &&
      !form.http_embedding_model_ref_id
    ) {
      alert("HTTP stream service 启用记忆库时需要绑定文本向量模型");
      return;
    }
    if (form.id) {
      await system.services.update(form.id, payload);
    } else {
      await system.services.create(payload);
    }
    closeEditor();
    await load();
  } catch (error) {
    alert(`保存 Agent 失败: ${(error as Error).message}`);
  }
}

async function removeService(id: string) {
  if (!window.confirm("确认删除这个 Agent 吗？")) {
    return;
  }
  await system.services.delete(id);
  if (form.id === id) {
    closeEditor();
  }
  await load();
}

async function startAgent(id: string) {
  try {
    console.log(`[Agent] 启动 Agent ${id}`);
    await system.services.start(id);
    await load();
  } catch (error) {
    alert(`启动失败: ${(error as Error).message}`);
  }
}

async function stopAgent(id: string) {
  try {
    console.log(`[Agent] 停止 Agent ${id}`);
    await system.services.stop(id);
    await load();
  } catch (error) {
    alert(`停止失败: ${(error as Error).message}`);
  }
}

async function toggleServiceRuntime(service: ServiceWithRuntime) {
  if (service.runtime.status === "running") {
    await stopAgent(service.config_id);
  } else {
    await startAgent(service.config_id);
  }
}

function summarizeService(
  service: ServiceWithRuntime,
): Array<{ label: string; value: string; mono?: boolean }> {
  const items: Array<{ label: string; value: string; mono?: boolean }> = [
    { label: "Config ID", value: compactId(service.config_id), mono: true },
    { label: "模型", value: llmName(service), mono: false },
    { label: "自动启动", value: service.auto_start ? "开启" : "关闭" },
  ];
  if (service.runtime.instance_id) {
    items.push({
      label: "Instance ID",
      value: compactId(service.runtime.instance_id),
      mono: true,
    });
  }
  const serviceType = service.agent_type as Record<string, unknown>;
  if (service.agent_type.type === "qq_chat") {
    items.push(
      {
        label: "Bot Adapter",
        value:
          connectionName(
            String(serviceType.ims_bot_adapter_connection_id ?? ""),
          ) || "未绑定",
      },
      {
        label: "Bot QQ",
        value: String(service.qq_chat_profile?.bot_user_id ?? "") || "未知",
      },
      {
        label: "RustFS",
        value:
          connectionName(String(serviceType.rustfs_connection_id ?? "")) ||
          "未绑定",
      },
      {
        label: "Web Search",
        value:
          connectionName(
            String(serviceType.web_search_engine_connection_id ?? ""),
          ) || "未绑定",
      },
      {
        label: "Bot Name",
        value: String(serviceType.bot_name ?? "") || "未设置",
      },
      {
        label: "图片理解模型",
        value:
          llmRefName(String(serviceType.image_understand_llm_ref_id ?? "")) ||
          llmRefName(String(serviceType.llm_ref_id ?? "")) ||
          "未绑定",
      },
      {
        label: "数学编程模型",
        value:
          llmRefName(String(serviceType.math_programming_llm_ref_id ?? "")) ||
          llmName(agent),
      },
      {
        label: "自然语言回复模型",
        value:
          llmRefName(
            String(serviceType.natural_language_reply_llm_ref_id ?? ""),
          ) || "未绑定",
      },
      {
        label: "文本向量模型",
        value:
          llmRefName(String(serviceType.embedding_model_ref_id ?? "")) ||
          "未绑定",
      },
      {
        label: "记忆 Weaviate",
        value:
          connectionName(
            String(serviceType.weaviate_memory_connection_id ?? ""),
          ) || "未绑定",
      },
      {
        label: "分词 Tokenizer",
        value:
          connectionName(String(serviceType.tokenizer_connection_id ?? "")) ||
          "未绑定",
      },
      {
        label: "System Prompt",
        value: String(serviceType.system_prompt ?? "").trim()
          ? "已配置"
          : "未设置",
      },
      {
        label: "Reply Prompt",
        value: String(
          serviceType.natural_language_reply_system_prompt ?? "",
        ).trim()
          ? "已配置"
          : "未设置",
      },
      {
        label: "Max Message",
        value: String(serviceType.max_message_length ?? 500),
      },
      { label: "Max Steer", value: String(serviceType.max_steer_count ?? 4) },
      {
        label: "Emotion Dims",
        value: String(
          (serviceType.emotion_dimensions as unknown[] | undefined)?.length ?? 0,
        ),
      },
    );
  } else if (service.agent_type.type === "http_stream") {
    items.push(
      {
        label: "Bind",
        value: String(serviceType.bind ?? "127.0.0.1:18080"),
        mono: true,
      },
      {
        label: "API Key",
        value: String(serviceType.api_key ?? "") ? "已配置" : "未设置",
      },
      {
        label: "Web Search",
        value:
          connectionName(
            String(serviceType.web_search_engine_connection_id ?? ""),
          ) || "未绑定",
      },
      {
        label: "记忆向量模型",
        value:
          llmRefName(String(serviceType.embedding_model_ref_id ?? "")) ||
          "未绑定",
      },
      {
        label: "记忆 Weaviate",
        value:
          connectionName(
            String(serviceType.weaviate_memory_connection_id ?? ""),
          ) || "未绑定",
      },
      {
        label: "web_search",
        value:
          (
            serviceType.default_tools_enabled as
              | Record<string, unknown>
              | undefined
          )?.web_search === false
            ? "关闭"
            : "开启",
      },
    );
  } else {
    items.push(
      {
        label: "工作模式",
        value: "Dashboard Session Workspace",
      },
      {
        label: "create_file",
        value:
          (
            serviceType.default_tools_enabled as
              | Record<string, unknown>
              | undefined
          )?.create_file === false
            ? "关闭"
            : "开启",
      },
      {
        label: "exec_cmd",
        value:
          (
            serviceType.default_tools_enabled as
              | Record<string, unknown>
              | undefined
          )?.exec_cmd === false
            ? "关闭"
            : "开启",
      },
    );
  }
  if (service.runtime.last_error) {
    items.push({ label: "最近错误", value: service.runtime.last_error });
  }
  return items;
}

function connectionName(id: string): string {
  return connections.value.find((item) => item.config_id === id)?.name ?? "";
}

function llmName(service: ServiceWithRuntime): string {
  const serviceType = service.agent_type as Record<string, unknown>;
  const llmId = String(serviceType.llm_ref_id ?? "");
  return llmRefName(llmId) || "未绑定";
}

function llmRefName(id: string): string {
  return llm.value.find((item) => item.config_id === id)?.name ?? "";
}

function botAvatarUrl(service: ServiceWithRuntime): string {
  // QQ Chat Agent Service 使用 bot_avatar_url
  if (service.agent_type.type === "qq_chat") {
    return String(service.qq_chat_profile?.bot_avatar_url ?? "");
  }
  // HTTP Stream 和 Workspace Agent Service 使用 avatar_url
  return String(service.avatar_url ?? "");
}

function runtimeBadgeText(service: ServiceWithRuntime): string {
  switch (service.runtime.status) {
    case "running":
      return service.runtime.instance_id
        ? `已启动 (${summarizeIds([service.runtime.instance_id])})`
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

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`Agent 页面加载失败: ${(error as Error).message}`);
  });
});
</script>

<style scoped lang="scss">
.service-loading-state {
  min-height: 180px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--admin-subtle);
}

.service-loading-spinner {
  width: 18px;
  height: 18px;
  border: 2px solid color-mix(in srgb, var(--admin-accent) 28%, transparent);
  border-top-color: var(--admin-accent);
  border-radius: 50%;
  animation: agent-loading-spin 0.75s linear infinite;
  flex-shrink: 0;
}

@keyframes agent-loading-spin {
  to {
    transform: rotate(360deg);
  }
}

/* ── Emotion Dimensions Modal ── */

.emotion-dim-modal {
  max-width: 760px;
}

.emotion-dim-modal-header {
  flex-shrink: 0;
  padding: 16px 28px;
  border-bottom: 1px solid var(--admin-border);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.emotion-dim-close-btn {
  width: 32px;
  height: 32px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 8px;
  background: color-mix(in srgb, #d9534f 16%, transparent);
  color: #d9534f;
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
  flex-shrink: 0;

  &:hover {
    background: #d9534f;
    color: #fff;
  }
}

/* ── Dimension Card ── */

.emotion-dim-card {
  background: color-mix(in srgb, var(--admin-bg-panel-strong) 60%, transparent);
  border: 1px solid var(--admin-border);
  border-radius: 12px;
  padding: 16px 18px;
  margin-bottom: 10px;
  transition: border-color 0.15s;

  &--editing {
    border-color: var(--admin-accent, #5b8def);
    background: color-mix(in srgb, var(--admin-accent, #5b8def) 6%, transparent);
  }
}

.emotion-dim-card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

/* ── Progress Bars ── */

.emotion-dim-bars {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.emotion-dim-bar-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.emotion-dim-bar-label {
  width: 48px;
  flex-shrink: 0;
  font-size: 12px;
  color: var(--admin-muted);
  text-align: right;
}

.emotion-dim-bar-track {
  flex: 1;
  height: 8px;
  border-radius: 4px;
  background: color-mix(in srgb, var(--admin-border) 80%, transparent);
  overflow: hidden;
}

.emotion-dim-bar-fill {
  height: 100%;
  border-radius: 4px;
  transition: width 0.2s ease;

  &--increase {
    background: linear-gradient(90deg, #4caf50, #81c784);
  }

  &--decrease {
    background: linear-gradient(90deg, #ff7043, #ffab91);
  }
}

.emotion-dim-bar-value {
  width: 32px;
  flex-shrink: 0;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  color: var(--admin-ink);
  text-align: right;
}

/* ── Inline weight inputs in editing card ── */

.emotion-dim-weight-value {
  display: inline-block;
  min-width: 32px;
  text-align: center;
  font-variant-numeric: tabular-nums;
  font-size: 13px;
  color: var(--admin-ink);
}

.emotion-dim-card-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 12px;
}

/* ── Range input styling ── */

.emotion-dim-card input[type="range"] {
  -webkit-appearance: none;
  appearance: none;
  width: 100%;
  height: 6px;
  border-radius: 3px;
  background: color-mix(in srgb, var(--admin-border) 80%, transparent);
  outline: none;
  cursor: pointer;
}

.emotion-dim-card input[type="range"]::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--admin-accent, #5b8def);
  border: 2px solid var(--admin-bg-panel);
  cursor: pointer;
  transition: transform 0.1s;

  &:hover {
    transform: scale(1.15);
  }
}

.emotion-dim-card input[type="range"]::-moz-range-thumb {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--admin-accent, #5b8def);
  border: 2px solid var(--admin-bg-panel);
  cursor: pointer;
}

/* ── Prompt display in dimension card ── */

.emotion-dim-prompts {
  border-top: 1px dashed var(--admin-border);
  padding-top: 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.emotion-dim-prompt-line {
  display: flex;
  gap: 8px;
  font-size: 12px;
  line-height: 1.4;
}

.emotion-dim-prompt-label {
  flex-shrink: 0;
  width: 32px;
  text-align: right;
  color: var(--admin-muted);
}

/* ── Avatar Upload ── */

.avatar-upload-row {
  display: flex;
  align-items: center;
  gap: 16px;
}

.avatar-preview,
.avatar-placeholder {
  width: 64px;
  height: 64px;
  border-radius: 8px;
  object-fit: cover;
  flex-shrink: 0;
}

.avatar-preview {
  border: 1px solid var(--admin-border);
}

.avatar-placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--admin-accent);
  color: #fff;
  font-size: 24px;
  font-weight: 600;
}

.avatar-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.emotion-dim-prompt-text {
  color: color-mix(in srgb, var(--admin-ink) 74%, transparent);
}

/* ── Service Edit Modal ── */

.service-edit-modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: grid;
  place-items: center;
  padding: 0;
  overflow: hidden;
  background: color-mix(in srgb, var(--bg) 55%, transparent 45%);
  backdrop-filter: blur(12px);
}

.service-edit-modal {
  width: 85vw;
  height: 85vh;
  display: flex;
  flex-direction: column;
  padding: 0;
  border-radius: 24px;
  border: 1px solid var(--admin-border);
  background: linear-gradient(180deg, color-mix(in srgb, var(--admin-bg-panel) 94%, transparent 6%), color-mix(in srgb, var(--admin-bg-panel-strong) 98%, transparent 2%));
  box-shadow: var(--admin-card-shadow);
  overflow: hidden;
}

.service-edit-modal-header {
  flex-shrink: 0;
  padding: 20px 28px;
  border-bottom: 1px solid var(--admin-border);
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.service-edit-modal-body {
  flex: 1;
  overflow-y: auto;
  padding: 24px 28px;
}

.service-edit-modal-body::-webkit-scrollbar {
  width: 10px;
}

.service-edit-modal-body::-webkit-scrollbar-track {
  background: transparent;
}

.service-edit-modal-body::-webkit-scrollbar-thumb {
  background: var(--admin-border);
  border-radius: 5px;
  border: 2px solid var(--admin-bg-panel);
}

.service-edit-modal-body::-webkit-scrollbar-thumb:hover {
  background: var(--admin-muted);
}

.service-edit-modal-footer {
  flex-shrink: 0;
  padding: 16px 28px;
  border-top: 1px solid var(--admin-border);
  display: flex;
  justify-content: flex-end;
  gap: 12px;
}

@media (max-width: 900px) {
  .service-edit-modal {
    width: 95vw;
    height: 90vh;
    border-radius: 16px;
  }

  .service-edit-modal-header {
    padding: 16px 20px;
  }

  .service-edit-modal-body {
    padding: 16px 20px;
  }

  .service-edit-modal-footer {
    padding: 12px 20px;
  }
}
</style>
