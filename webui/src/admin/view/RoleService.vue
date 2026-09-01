<template>
  <section class="page agent-service-page">
    <AdminPageHeader title="Service 管理">
      <t-button variant="outline" @click="triggerServiceImportFile">导入配置</t-button>
      <input ref="serviceImportFileInput" type="file" accept=".json" class="agent-service-import-input" @change="handleServiceFileChange" />
      <t-button theme="primary" @click="startCreate">新建 Service</t-button>
    </AdminPageHeader>

    <!-- 新建 Service 抽屉 -->
    <t-drawer
      v-model:visible="showCreatePicker"
      size="960px"
      :close-on-overlay-click="false"
      :footer="false"
      @close="closeCreatePicker"
    >
      <template #header>
        <div class="agent-service-create-drawer-header">
          <strong>{{ showCreateForm ? '新建 Service' : '选择 Service 类型' }}</strong>
          <t-tooltip content="关闭">
            <t-button variant="text" shape="square" aria-label="关闭" @click="closeCreatePicker">
              <CloseIcon />
            </t-button>
          </t-tooltip>
        </div>
      </template>
      <div v-if="showCreateForm" class="agent-service-drawer-body">
        <t-form class="agent-service-form" label-align="top">
          <!-- RoleService 配置 -->
          <t-card class="agent-service-form-section" :bordered="false">
            <template #title>{{ form.type === 'qq_chat' ? 'RoleService 配置' : '基本信息' }}</template>
            <div class="agent-service-form-grid">
              <t-form-item label="名称" required>
                <t-input v-model="form.name" />
              </t-form-item>
              <t-form-item label="类型">
                <t-select v-model="form.type" disabled>
                  <t-option value="qq_chat" label="QQ Chat RoleService" />
                  <t-option value="workspace" label="Workspace RoleService" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="form.type === 'qq_chat'" label="Bot Adapter" required>
                <t-select v-model="form.ims_bot_adapter_connection_id" placeholder="请选择">
                  <t-option value="" label="请选择" />
                  <t-option v-for="item in botConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="form.type === 'qq_chat'" label="Bot 名称">
                <t-input v-model="form.bot_name" placeholder="用户与 Bot 对话时显示的名称" />
              </t-form-item>
            </div>
            <div class="agent-service-check-row">
              <t-checkbox v-model="form.enabled">启用</t-checkbox>
              <t-checkbox v-model="form.auto_start">开机自动启动</t-checkbox>
              <t-checkbox v-if="form.type === 'workspace'" v-model="form.is_default">默认 Service</t-checkbox>
            </div>
            <t-form-item v-if="form.type === 'workspace'" label="头像" class="agent-service-form-item-full">
              <div class="agent-service-avatar-row">
                <img v-if="form.avatar_url" :src="getAvatarDisplayUrl(form.avatar_url)" alt="Avatar preview" class="agent-service-avatar-preview" />
                <div v-else class="agent-service-avatar-placeholder">{{ form.name ? form.name.slice(0, 1).toUpperCase() : 'A' }}</div>
                <div class="agent-service-avatar-actions">
                  <input ref="createAvatarFileInput" type="file" accept="image/*" style="display: none" @change="handleAvatarFileSelect" />
                  <t-button variant="text" @click="$refs.createAvatarFileInput?.click()">{{ form.avatar_url ? '更换头像' : '上传头像' }}</t-button>
                  <t-button v-if="form.avatar_url" variant="text" theme="danger" @click="clearAvatar">删除</t-button>
                </div>
              </div>
              <t-input v-model="form.avatar_url" placeholder="头像 URL（可选，或直接上传图片）" style="margin-top: 8px" />
            </t-form-item>
          </t-card>

          <!-- 模型配置 -->
          <t-card class="agent-service-form-section" :bordered="false">
            <template #title>{{ form.type === 'workspace' ? '默认模型' : '模型配置' }}</template>
            <div class="agent-service-form-grid">
              <t-form-item :label="form.type === 'workspace' ? '默认模型' : '主 Brain 模型'" required>
                <t-select v-model="form.llm_ref_id" placeholder="请选择" @change="handlePrimaryModelChange">
                  <t-option class="agent-service-add-model-option" value="__add_model__" label="新增模型配置">
                    <span class="agent-service-add-model-option-content"><AddIcon />新增模型配置</span>
                  </t-option>
                  <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="form.type === 'workspace'" label="AGENTS.md">
                <t-checkbox v-model="form.agents_md_enabled">关注 AGENTS.md</t-checkbox>
                <div class="agent-service-form-hint">让Agent关注 AGENTS.md</div>
              </t-form-item>
              <t-form-item v-if="form.type === 'workspace'" label="Agent 记忆">
                <t-checkbox v-model="form.workspace_memory_enabled">启用 Agent 记忆</t-checkbox>
                <div class="agent-service-form-hint">启用后 Agent 会回想或者记忆对话中的信息，需要记忆库的支持</div>
              </t-form-item>
              <t-form-item v-if="form.type === 'qq_chat'" label="数学/编程模型">
                <t-select v-model="form.math_programming_llm_ref_id" placeholder="回退主 Brain 模型" clearable>
                  <t-option value="" label="回退主 Brain 模型" />
                  <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="form.type === 'qq_chat'" label="Preprompt 模型">
                <t-select v-model="form.intent_classification_llm_ref_id" placeholder="回退主 Brain 模型" clearable>
                  <t-option value="" label="回退主 Brain 模型" />
                  <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="form.type === 'qq_chat'" label="自然语言回复模型">
                <t-select v-model="form.natural_language_reply_llm_ref_id" placeholder="请选择" clearable>
                  <t-option value="" label="请选择" />
                  <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="form.type === 'qq_chat'" label="分词配置">
                <t-select v-model="form.tokenizer_connection_id" placeholder="不使用（标点分段）" clearable>
                  <t-option value="" label="不使用（标点分段）" />
                  <t-option v-for="item in tokenizerConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
            </div>
          </t-card>

          <t-card v-if="form.type === 'workspace' && (form.workspace_memory_enabled || form.default_tools_enabled.web_search)" class="agent-service-form-section" :bordered="false">
            <template #title>检索增强生成</template>
            <div class="agent-service-form-grid">
              <t-form-item v-if="form.workspace_memory_enabled" label="记忆介质" required :status="!form.workspace_memory_backend ? 'error' : undefined" :help="!form.workspace_memory_backend ? '启用 Agent 记忆后必须选择记忆介质。' : undefined">
                <t-select v-model="form.workspace_memory_backend" placeholder="请选择记忆库" @change="handleMemoryBackendChange">
                  <t-option class="agent-service-add-retrieval-option" value="__add_retrieval_database__" label="新增检索数据库">
                    <span class="agent-service-add-model-option-content"><AddIcon />新增检索数据库</span>
                  </t-option>
                  <t-option value="local_file" label="本地文件" />
                  <t-option value="weaviate" label="Weaviate" />
                  <t-option value="elasticsearch" label="Elasticsearch" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="form.workspace_memory_backend === 'weaviate' || form.workspace_memory_backend === 'elasticsearch'" label="文本向量模型" required :status="!form.workspace_embedding_model_ref_id ? 'error' : undefined" :help="!form.workspace_embedding_model_ref_id ? '必须选择文本向量模型。' : undefined">
                <t-select v-model="form.workspace_embedding_model_ref_id" placeholder="请选择文本向量模型">
                  <t-option v-for="item in embeddingModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="form.workspace_memory_backend === 'weaviate'" label="Weaviate Memory Connection" required :status="!form.workspace_weaviate_memory_connection_id ? 'error' : undefined" :help="!form.workspace_weaviate_memory_connection_id ? '必须选择记忆库连接。' : undefined">
                <t-select v-model="form.workspace_weaviate_memory_connection_id" placeholder="请选择记忆库连接">
                  <t-option v-for="item in memoryWeaviateConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="form.workspace_memory_backend === 'elasticsearch'" label="Elasticsearch Memory Connection" required :status="!form.workspace_elasticsearch_memory_connection_id ? 'error' : undefined" :help="!form.workspace_elasticsearch_memory_connection_id ? '必须选择记忆库连接。' : undefined">
                <t-select v-model="form.workspace_elasticsearch_memory_connection_id" placeholder="请选择记忆库连接">
                  <t-option v-for="item in memoryElasticsearchConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="form.default_tools_enabled.web_search" label="Web Search Engine" required :status="!form.web_search_engine_connection_id ? 'error' : undefined" :help="!form.web_search_engine_connection_id ? '启用联网搜索后必须选择连接。' : undefined">
                <t-select v-model="form.web_search_engine_connection_id" placeholder="请选择" @change="handleWebSearchChange">
                  <t-option class="agent-service-add-web-search-option" value="__add_web_search__" label="新增 Web Search">
                    <span class="agent-service-add-model-option-content"><AddIcon />新增 Web Search</span>
                  </t-option>
                  <t-option value="" label="请选择" />
                  <t-option v-for="item in webSearchEngineConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
            </div>
          </t-card>

          <!-- QQ Chat 专属字段 -->
          <template v-if="form.type === 'qq_chat'">
            <t-card class="agent-service-form-section" :bordered="false">
              <template #title>RAG 配置</template>
              <div class="agent-service-form-grid">
                <t-form-item label="关系型数据库">
                  <t-select v-model="form.rdb_id" placeholder="不使用" clearable>
                    <t-option value="" label="不使用" />
                    <t-option v-for="item in taskDbConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                  </t-select>
                </t-form-item>
                <t-form-item label="RustFS">
                  <t-select v-model="form.rustfs_connection_id" placeholder="不使用" clearable>
                    <t-option value="" label="不使用" />
                    <t-option v-for="item in rustfsConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                  </t-select>
                </t-form-item>
                <t-form-item label="Web Search Engine" required>
                  <t-select v-model="form.web_search_engine_connection_id" placeholder="请选择" @change="handleWebSearchChange">
                    <t-option class="agent-service-add-web-search-option" value="__add_web_search__" label="新增 Web Search">
                      <span class="agent-service-add-model-option-content"><AddIcon />新增 Web Search</span>
                    </t-option>
                    <t-option value="" label="请选择" />
                    <t-option v-for="item in webSearchEngineConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                  </t-select>
                </t-form-item>
                <t-form-item label="检索数据库">
                  <t-select v-model="form.retrieval_store_id" placeholder="不使用" clearable>
                    <t-option value="" label="不使用" />
                    <t-option value="__local_markdown__" label="本地 Markdown" />
                    <t-option v-for="item in retrievalConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                  </t-select>
                </t-form-item>
                <t-form-item label="Embedding 模型">
                  <t-select v-model="form.embedding_model_ref_id" placeholder="不使用" clearable>
                    <t-option value="" label="不使用" />
                    <t-option v-for="item in embeddingModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                  </t-select>
                </t-form-item>
                <t-form-item v-if="false" label="Weaviate 图片检索连接">
                  <t-select v-model="form.weaviate_image_connection_id" placeholder="不使用" clearable>
                    <t-option value="" label="不使用" />
                    <t-option v-for="item in imageWeaviateConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                  </t-select>
                </t-form-item>
                <t-form-item v-if="false" label="Weaviate 记忆连接">
                  <t-select v-model="form.weaviate_memory_connection_id" placeholder="不使用" clearable>
                    <t-option value="" label="不使用" />
                    <t-option v-for="item in memoryWeaviateConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                  </t-select>
                </t-form-item>
                <t-form-item v-if="false" label="Elasticsearch 图片检索连接">
                  <t-select v-model="form.elasticsearch_image_connection_id" placeholder="不使用" clearable>
                    <t-option value="" label="不使用" />
                    <t-option v-for="item in imageElasticsearchConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                  </t-select>
                </t-form-item>
                <t-form-item v-if="false" label="Elasticsearch 记忆连接">
                  <t-select v-model="form.elasticsearch_memory_connection_id" placeholder="不使用" clearable>
                    <t-option value="" label="不使用" />
                    <t-option v-for="item in memoryElasticsearchConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                  </t-select>
                </t-form-item>
              </div>
            </t-card>

            <t-card class="agent-service-form-section" :bordered="false">
              <template #title>Prompt engineering</template>
              <div class="agent-service-form-grid">
                <t-form-item label="System Prompt" class="agent-service-form-item-full">
                  <t-textarea v-model="form.system_prompt" placeholder="可选。会追加在 QQ Chat RoleService 的通用系统规则后面。" />
                </t-form-item>
                <t-form-item label="自然语言回复 Prompt" class="agent-service-form-item-full">
                  <t-textarea v-model="form.natural_language_reply_system_prompt" placeholder="可选。专门给自然语言回复模型使用的系统提示词。" />
                </t-form-item>
              </div>
            </t-card>

            <t-card class="agent-service-form-section" :bordered="false">
              <template #title>行为控制</template>
              <div class="agent-service-form-grid">
                <t-form-item label="最长输出消息长度">
                  <t-input-number v-model="form.max_message_length" :min="1" />
                </t-form-item>
                <t-form-item label="用户最多 Steer 次数">
                  <t-input-number v-model="form.max_steer_count" :min="0" />
                  <div class="agent-service-form-hint">当 Service 还没发出最终回复时，用户继续发消息会被视为"插嘴 / steer"。这里控制单次活跃回复流程里最多接受多少次插嘴；默认 4 次，超出会被丢弃并写入日志。</div>
                </t-form-item>
                <t-form-item label="Dream">
                  <t-checkbox v-model="form.dream_enabled">启用 Dream 记忆</t-checkbox>
                  <div v-if="form.dream_enabled" class="agent-service-form-grid" style="margin-top: 8px">
                    <t-input-number v-model="form.dream_interval_value" :min="1" />
                    <t-select v-model="form.dream_interval_unit">
                      <t-option value="minutes" label="分" />
                      <t-option value="hours" label="时" />
                      <t-option value="days" label="天" />
                    </t-select>
                  </div>
                  <div v-if="form.dream_enabled && !form.rdb_id" class="agent-service-form-hint">Dream 需要配置关系数据库连接。</div>
                </t-form-item>
              </div>
              <div class="agent-service-form-grid">
                <t-form-item label="情绪维度" class="agent-service-form-item-full">
                  <div class="agent-service-form-hint">Service 的情绪可以由一个或者多个维度组成，这些维度共同构成 Agent 的决策、行为和输出语言风格。</div>
                  <div class="agent-service-form-hint" style="margin-top: 6px">当前已配置 {{ form.emotion_dimensions.length }} 个维度。</div>
                  <t-button variant="text" style="margin-top: 6px; padding-left: 0" @click="openEmotionDimensionsModal">配置情绪维度</t-button>
                </t-form-item>
                <t-form-item label="Rate Limit" class="agent-service-form-item-full">
                  <div class="agent-service-form-hint">按 N 天/小时/分钟限制调用次数，计数跟随用户（跨群与私聊共享），优先级：用户 &gt; 群组 &gt; 默认。</div>
                  <t-button variant="text" style="margin-top: 6px; padding-left: 0" @click="openRateLimitModal">编辑 Rate Limit</t-button>
                </t-form-item>
                <t-form-item label="Ignore Rules" class="agent-service-form-item-full">
                  <div class="agent-service-form-hint">命中后仅做消息存储，不回复、不进入推理流程。</div>
                  <t-button variant="text" style="margin-top: 6px; padding-left: 0" :disabled="Boolean(ignoreRulesDisabledReason)" @click="openIgnoreRulesModal()">管理 Ignore Rules</t-button>
                  <div v-if="ignoreRulesDisabledReason" class="agent-service-form-hint" style="margin-top: 4px">
                    <InfoCircleIcon /> {{ ignoreRulesDisabledReason }}
                  </div>
                </t-form-item>
              </div>
            </t-card>
          </template>

          <!-- 默认工具 -->
          <t-card v-if="currentDefaultTools.length > 0" class="agent-service-form-section" :bordered="false">
            <template #title>工具和能力</template>
            <div class="agent-service-default-tools-search">
              <t-input v-model="defaultToolSearchQuery" placeholder="搜索工具" clearable>
                <template v-if="defaultToolSearchQuery" #suffixIcon>
                  <t-button variant="text" size="small" @click="defaultToolSearchQuery = ''">清空</t-button>
                </template>
              </t-input>
            </div>
            <div v-if="filteredDefaultTools.length === 0" class="agent-service-empty-state">没有匹配的工具。</div>
            <t-table v-else :data="filteredDefaultTools" :columns="defaultToolColumns" :hover="true" :pagination="false" row-key="id" table-layout="fixed">
              <template #enabled="{ row }">
                <t-checkbox v-model="form.default_tools_enabled[row.id]" />
              </template>
              <template #edit="{ row }">
                <t-button variant="text" size="small" @click="openDefaultToolEditModal(row.id)">编辑</t-button>
              </template>
            </t-table>
          </t-card>

          <!-- 工具调用上限回文 -->
          <t-card v-if="form.type === 'qq_chat'" class="agent-service-form-section" :bordered="false">
            <template #title>工具调用上限回文</template>
            <t-form-item label="达到调用上限回文（可选）" class="agent-service-form-item-full">
              <t-textarea v-model="form.tool_session_limit_message" placeholder="当前工具已经达到 [{limit_scope}]的调用次数限制，但是你不要把这个直接当成自然语言输出给用户，仅当用户问及详细原因时，再告知" />
            </t-form-item>
            <div class="agent-service-form-hint">留空则使用默认提示。可用 {limit_scope} 占位符表示限制范围（会替换为"单次会话"或"用户"）。</div>
          </t-card>

          <!-- 工具配置 -->
          <t-card class="agent-service-form-section" :bordered="false">
            <template #title>
              <div class="agent-service-section-title-row">
                <span>工具配置</span>
                <t-button variant="text" @click="addTool">新增工具</t-button>
              </div>
            </template>
            <div v-if="form.tools.length === 0" class="agent-service-empty-state">还没有配置工具。</div>
            <div v-for="(tool, index) in form.tools" :key="tool.id" class="agent-service-tool-block">
              <t-card :bordered="true" style="margin-top: 12px">
                <template #title>
                  <div class="agent-service-tool-header">
                    <strong>工具 {{ index + 1 }}</strong>
                    <t-button variant="text" theme="danger" size="small" @click="removeTool(index)">移除</t-button>
                  </div>
                </template>
                <div class="agent-service-form-grid">
                  <t-form-item label="ID">
                    <t-input v-model="tool.id" />
                  </t-form-item>
                  <t-form-item label="名称">
                    <t-input v-model="tool.name" />
                  </t-form-item>
                  <t-form-item label="描述" class="agent-service-form-item-full">
                    <t-input v-model="tool.description" />
                  </t-form-item>
                  <t-form-item label="运行时长">
                    <t-select v-model="tool.runDuration">
                      <t-option value="Short" label="Short（短时）" />
                      <t-option value="Long" label="Long（长时）" />
                    </t-select>
                  </t-form-item>
                  <t-form-item label="工具模式">
                    <t-select v-model="tool.implementation">
                      <t-option value="node_graph" label="node_graph" />
                      <t-option value="python_script" label="python_script" />
                    </t-select>
                  </t-form-item>
                  <t-form-item v-if="tool.implementation === 'node_graph'" label="目标类型">
                    <t-select v-model="tool.targetType" @change="handleToolTargetTypeChange(tool)">
                      <t-option value="workflow_set" label="workflow_set" />
                      <t-option value="file_path" label="file_path" />
                      <t-option value="inline_graph" label="inline_graph" />
                    </t-select>
                  </t-form-item>
                  <t-form-item style="align-self: end">
                    <t-checkbox v-model="tool.enabled">启用该工具</t-checkbox>
                  </t-form-item>
                  <t-form-item v-if="form.type === 'qq_chat' && tool.enabled" label="单次会话调用上限">
                    <t-input-number v-model="form.tool_session_call_limits[tool.name]" :min="0" placeholder="不限制" />
                    <div class="agent-service-form-hint" style="font-size: 12px">0 或留空表示不限制</div>
                  </t-form-item>
                  <t-form-item v-if="tool.implementation === 'node_graph' && tool.targetType === 'workflow_set'" label="Workflow Set 名称" class="agent-service-form-item-full">
                    <t-select v-model="tool.workflowName" @change="applyWorkflowSetMetadata(tool)" placeholder="请选择">
                      <t-option v-for="workflow in workflows" :key="workflow.name" :value="workflow.name" :label="workflow.display_name || workflow.name" />
                    </t-select>
                  </t-form-item>
                  <t-form-item v-else-if="tool.implementation === 'node_graph' && tool.targetType === 'file_path'" label="文件路径" class="agent-service-form-item-full">
                    <t-input v-model="tool.filePath" placeholder="workflow_set/demo.json" />
                  </t-form-item>
                  <t-form-item v-else-if="tool.implementation === 'node_graph'" label="Inline Graph JSON" class="agent-service-form-item-full">
                    <t-textarea v-model="tool.inlineGraphJson" />
                  </t-form-item>
                  <t-form-item v-else label="Python 脚本路径" class="agent-service-form-item-full">
                    <t-input v-model="tool.pythonScriptPath" placeholder="utils/python_tools/echo_tool.py" />
                  </t-form-item>
                  <t-form-item v-if="tool.implementation === 'python_script'" label="入口函数名">
                    <t-input v-model="tool.pythonModuleEntry" placeholder="run_tool" />
                  </t-form-item>
                  <t-form-item v-if="tool.implementation === 'python_script'" label="Python 运行时">
                    <t-select v-model="tool.pythonMode">
                      <t-option value="inherit" label="继承全局设置" />
                      <t-option value="uv_project" label="uv_project" />
                      <t-option value="project_venv" label="project_venv" />
                      <t-option value="custom_executable" label="custom_executable" />
                    </t-select>
                  </t-form-item>
                  <t-form-item v-if="tool.implementation === 'python_script' && tool.pythonMode === 'custom_executable'" label="自定义 Python 路径" class="agent-service-form-item-full">
                    <t-input v-model="tool.pythonExecutablePath" placeholder="C:\\Python311\\python.exe" />
                  </t-form-item>
                  <t-form-item v-if="tool.implementation === 'python_script'" label="超时（秒）">
                    <t-input-number v-model="tool.pythonTimeoutSecs" :min="1" />
                  </t-form-item>
                  <t-form-item label="Parameters JSON" class="agent-service-form-item-full">
                    <template #label>
                      <div class="agent-service-params-label">
                        <span>Parameters JSON</span>
                        <t-button v-if="tool.implementation === 'node_graph' && tool.targetType === 'workflow_set' && tool.workflowName" variant="text" size="small" :disabled="syncingToolIndex === index" @click="syncToolFromGraph(tool, index)">
                          {{ syncingToolIndex === index ? '同步中…' : '从节点图更新' }}
                        </t-button>
                      </div>
                    </template>
                    <t-textarea v-model="tool.parametersJson" />
                  </t-form-item>
                  <t-form-item label="Outputs JSON" class="agent-service-form-item-full">
                    <t-textarea v-model="tool.outputsJson" />
                  </t-form-item>
                </div>
              </t-card>
            </div>
          </t-card>
        </t-form>

        <div class="agent-service-drawer-footer">
          <t-button variant="outline" @click="showCreateForm = false">返回</t-button>
          <t-button theme="primary" @click="submitForm">创建 Service</t-button>
        </div>
      </div>

      <div v-else class="agent-service-type-grid">
        <t-button v-for="type in serviceTypes" :key="type.value" variant="outline" class="agent-service-type-card" @click="pickCreateType(type.value)">
          <div class="agent-service-type-card-content">
            <strong>{{ type.label }}</strong>
            <span class="agent-service-type-hint">{{ type.hint }}</span>
          </div>
        </t-button>
      </div>
    </t-drawer>

    <!-- 编辑 Service 抽屉 -->
    <t-drawer
      v-model:visible="showEditModal"
      :header="form.name || '编辑 Service'"
      size="960px"
      :close-btn="true"
      :close-on-overlay-click="false"
      @close="closeEditModal"
    >
      <t-form class="agent-service-form" label-align="top">
        <!-- RoleService 配置 -->
        <t-card class="agent-service-form-section" :bordered="false">
          <template #title>{{ form.type === 'qq_chat' ? 'RoleService 配置' : '基本信息' }}</template>
          <div class="agent-service-form-grid">
            <t-form-item label="名称" required>
              <t-input v-model="form.name" />
            </t-form-item>
            <t-form-item label="类型">
              <t-select v-model="form.type" disabled>
                <t-option value="qq_chat" label="QQ Chat RoleService" />
                <t-option value="workspace" label="Workspace RoleService" />
              </t-select>
            </t-form-item>
            <t-form-item v-if="form.type === 'qq_chat'" label="Bot Adapter" required>
              <t-select v-model="form.ims_bot_adapter_connection_id" placeholder="请选择">
                <t-option value="" label="请选择" />
                <t-option v-for="item in botConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
              </t-select>
            </t-form-item>
            <t-form-item v-if="form.type === 'qq_chat'" label="Bot 名称">
              <t-input v-model="form.bot_name" placeholder="用户与 Bot 对话时显示的名称" />
            </t-form-item>
          </div>
          <div class="agent-service-check-row">
            <t-checkbox v-model="form.enabled">启用</t-checkbox>
            <t-checkbox v-model="form.auto_start">开机自动启动</t-checkbox>
            <t-checkbox v-if="form.type === 'workspace'" v-model="form.is_default">默认 Service</t-checkbox>
          </div>
          <t-form-item v-if="form.type === 'workspace'" label="头像" class="agent-service-form-item-full">
            <div class="agent-service-avatar-row">
              <img v-if="form.avatar_url" :src="getAvatarDisplayUrl(form.avatar_url)" alt="Avatar preview" class="agent-service-avatar-preview" />
              <div v-else class="agent-service-avatar-placeholder">{{ form.name ? form.name.slice(0, 1).toUpperCase() : 'A' }}</div>
              <div class="agent-service-avatar-actions">
                <input ref="avatarFileInput" type="file" accept="image/*" style="display: none" @change="handleAvatarFileSelect" />
                <t-button variant="text" @click="$refs.avatarFileInput?.click()">{{ form.avatar_url ? '更换头像' : '上传头像' }}</t-button>
                <t-button v-if="form.avatar_url" variant="text" theme="danger" @click="clearAvatar">删除</t-button>
              </div>
            </div>
            <t-input v-model="form.avatar_url" placeholder="头像 URL（可选，或直接上传图片）" style="margin-top: 8px" />
          </t-form-item>
        </t-card>

        <!-- 模型配置 -->
        <t-card class="agent-service-form-section" :bordered="false">
          <template #title>{{ form.type === 'workspace' ? '默认模型' : '模型配置' }}</template>
          <div class="agent-service-form-grid">
            <t-form-item :label="form.type === 'workspace' ? '默认模型' : '主 Brain 模型'" required>
              <t-select v-model="form.llm_ref_id" placeholder="请选择" @change="handlePrimaryModelChange">
                <t-option class="agent-service-add-model-option" value="__add_model__" label="新增模型配置">
                  <span class="agent-service-add-model-option-content"><AddIcon />新增模型配置</span>
                </t-option>
                <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
              </t-select>
            </t-form-item>
            <t-form-item v-if="form.type === 'workspace'" label="AGENTS.md">
              <t-checkbox v-model="form.agents_md_enabled">关注 AGENTS.md</t-checkbox>
              <div class="agent-service-form-hint">让 RoleService 的 Workspace Agent 关注 AGENTS.md</div>
            </t-form-item>
            <t-form-item v-if="form.type === 'workspace'" label="RoleService 记忆">
              <t-checkbox v-model="form.workspace_memory_enabled">启用 RoleService 记忆</t-checkbox>
              <div class="agent-service-form-hint">启用后 RoleService 的 Agent 会回想或者记忆对话中的信息，需要记忆库的支持。</div>
            </t-form-item>
            <t-form-item v-if="form.type === 'qq_chat'" label="数学/编程模型">
              <t-select v-model="form.math_programming_llm_ref_id" placeholder="回退主 Brain 模型" clearable>
                <t-option value="" label="回退主 Brain 模型" />
                <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
              </t-select>
            </t-form-item>
            <t-form-item v-if="form.type === 'qq_chat'" label="Preprompt 模型">
              <t-select v-model="form.intent_classification_llm_ref_id" placeholder="回退主 Brain 模型" clearable>
                <t-option value="" label="回退主 Brain 模型" />
                <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
              </t-select>
            </t-form-item>
            <t-form-item v-if="form.type === 'qq_chat'" label="自然语言回复模型">
              <t-select v-model="form.natural_language_reply_llm_ref_id" placeholder="请选择" clearable>
                <t-option value="" label="请选择" />
                <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
              </t-select>
            </t-form-item>
            <t-form-item v-if="form.type === 'qq_chat'" label="分词配置">
              <t-select v-model="form.tokenizer_connection_id" placeholder="不使用（标点分段）" clearable>
                <t-option value="" label="不使用（标点分段）" />
                <t-option v-for="item in tokenizerConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
              </t-select>
            </t-form-item>
          </div>
        </t-card>

        <t-card v-if="form.type === 'workspace' && (form.workspace_memory_enabled || form.default_tools_enabled.web_search)" class="agent-service-form-section" :bordered="false">
          <template #title>检索增强生成</template>
          <div class="agent-service-form-grid">
            <t-form-item v-if="form.workspace_memory_enabled" label="记忆库" required :status="!form.workspace_memory_backend ? 'error' : undefined" :help="!form.workspace_memory_backend ? '启用 Agent 记忆后必须选择记忆库。' : undefined">
              <t-select v-model="form.workspace_memory_backend" placeholder="请选择记忆库" @change="handleMemoryBackendChange">
                <t-option class="agent-service-add-retrieval-option" value="__add_retrieval_database__" label="新增检索数据库">
                  <span class="agent-service-add-model-option-content"><AddIcon />新增检索数据库</span>
                </t-option>
                <t-option value="local_file" label="本地 Markdown 文件" />
                <t-option value="weaviate" label="Weaviate" />
                <t-option value="elasticsearch" label="Elasticsearch" />
              </t-select>
            </t-form-item>
            <t-form-item v-if="form.workspace_memory_backend === 'weaviate' || form.workspace_memory_backend === 'elasticsearch'" label="文本向量模型" required :status="!form.workspace_embedding_model_ref_id ? 'error' : undefined" :help="!form.workspace_embedding_model_ref_id ? '必须选择文本向量模型。' : undefined">
              <t-select v-model="form.workspace_embedding_model_ref_id" placeholder="请选择文本向量模型">
                <t-option v-for="item in embeddingModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
              </t-select>
            </t-form-item>
            <t-form-item v-if="form.workspace_memory_backend === 'weaviate'" label="Weaviate Memory Connection" required :status="!form.workspace_weaviate_memory_connection_id ? 'error' : undefined" :help="!form.workspace_weaviate_memory_connection_id ? '必须选择记忆库连接。' : undefined">
              <t-select v-model="form.workspace_weaviate_memory_connection_id" placeholder="请选择记忆库连接">
                <t-option v-for="item in memoryWeaviateConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
              </t-select>
            </t-form-item>
            <t-form-item v-if="form.workspace_memory_backend === 'elasticsearch'" label="Elasticsearch Memory Connection" required :status="!form.workspace_elasticsearch_memory_connection_id ? 'error' : undefined" :help="!form.workspace_elasticsearch_memory_connection_id ? '必须选择记忆库连接。' : undefined">
              <t-select v-model="form.workspace_elasticsearch_memory_connection_id" placeholder="请选择记忆库连接">
                <t-option v-for="item in memoryElasticsearchConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
              </t-select>
            </t-form-item>
            <t-form-item v-if="form.default_tools_enabled.web_search" label="Web Search Engine" required :status="!form.web_search_engine_connection_id ? 'error' : undefined" :help="!form.web_search_engine_connection_id ? '启用联网搜索后必须选择连接。' : undefined">
              <t-select v-model="form.web_search_engine_connection_id" placeholder="请选择" @change="handleWebSearchChange">
                <t-option class="agent-service-add-web-search-option" value="__add_web_search__" label="新增 Web Search">
                  <span class="agent-service-add-model-option-content"><AddIcon />新增 Web Search</span>
                </t-option>
                <t-option value="" label="请选择" />
                <t-option v-for="item in webSearchEngineConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
              </t-select>
            </t-form-item>
          </div>
        </t-card>

        <!-- QQ Chat 专属 -->
        <template v-if="form.type === 'qq_chat'">
          <t-card v-if="false" class="agent-service-form-section" :bordered="false">
            <template #title>QQ Chat 模型配置</template>
            <div class="agent-service-form-grid">
              <t-form-item label="数学编程模型">
                <t-select v-model="form.math_programming_llm_ref_id" placeholder="回退主模型" clearable>
                  <t-option value="" label="回退主模型" />
                  <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item label="意图分类模型">
                <t-select v-model="form.intent_classification_llm_ref_id" placeholder="回退主模型" clearable>
                  <t-option value="" label="回退主模型" />
                  <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item label="自然语言回复模型">
                <t-select v-model="form.natural_language_reply_llm_ref_id" placeholder="请选择" clearable>
                  <t-option value="" label="请选择" />
                  <t-option v-for="item in chatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item label="自然语言回复 Prompt" class="agent-service-form-item-full">
                <t-textarea v-model="form.natural_language_reply_system_prompt" placeholder="可选。专门给自然语言回复模型使用的系统提示词。" />
              </t-form-item>
            </div>
          </t-card>

          <t-card v-if="false" class="agent-service-form-section" :bordered="false">
            <template #title>向量与分词</template>
            <div class="agent-service-form-grid">
              <t-form-item label="文本向量模型">
                <t-select v-model="form.embedding_model_ref_id" placeholder="不使用" clearable>
                  <t-option value="" label="不使用" />
                  <t-option v-for="item in embeddingModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item label="分词 Tokenizer 连接">
                <t-select v-model="form.tokenizer_connection_id" placeholder="不使用（标点分段）" clearable>
                  <t-option value="" label="不使用（标点分段）" />
                  <t-option v-for="item in tokenizerConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
            </div>
          </t-card>

          <t-card v-if="false" class="agent-service-form-section" :bordered="false">
            <template #title>Bot 配置</template>
            <div class="agent-service-form-grid">
              <t-form-item label="Bot Adapter" required>
                <t-select v-model="form.ims_bot_adapter_connection_id" placeholder="请选择">
                  <t-option value="" label="请选择" />
                  <t-option v-for="item in botConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item label="Bot Name">
                <t-input v-model="form.bot_name" />
              </t-form-item>
              <t-form-item label="System Prompt" class="agent-service-form-item-full">
                <t-textarea v-model="form.system_prompt" placeholder="可选。会追加在 QQ Chat RoleService 的通用系统规则后面。" />
              </t-form-item>
            </div>
          </t-card>

          <t-card class="agent-service-form-section" :bordered="false">
            <template #title>RAG 配置</template>
            <div class="agent-service-form-grid">
              <t-form-item label="RustFS">
                <t-select v-model="form.rustfs_connection_id" placeholder="不使用" clearable>
                  <t-option value="" label="不使用" />
                  <t-option v-for="item in rustfsConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item label="Web Search Engine" required>
                <t-select v-model="form.web_search_engine_connection_id" placeholder="请选择" @change="handleWebSearchChange">
                  <t-option class="agent-service-add-web-search-option" value="__add_web_search__" label="新增 Web Search">
                    <span class="agent-service-add-model-option-content"><AddIcon />新增 Web Search</span>
                  </t-option>
                  <t-option value="" label="请选择" />
                  <t-option v-for="item in webSearchEngineConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item label="关系型数据库">
                <t-select v-model="form.rdb_id" placeholder="不使用" clearable>
                  <t-option value="" label="不使用" />
                  <t-option v-for="item in taskDbConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item label="检索数据库">
                <t-select v-model="form.retrieval_store_id" placeholder="不使用" clearable>
                  <t-option value="" label="不使用" />
                  <t-option value="__local_markdown__" label="本地 Markdown" />
                  <t-option v-for="item in retrievalConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item label="Embedding 模型">
                <t-select v-model="form.embedding_model_ref_id" placeholder="不使用" clearable>
                  <t-option value="" label="不使用" />
                  <t-option v-for="item in embeddingModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="false" label="Weaviate 图片检索连接">
                <t-select v-model="form.weaviate_image_connection_id" placeholder="不使用" clearable>
                  <t-option value="" label="不使用" />
                  <t-option v-for="item in imageWeaviateConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="false" label="Weaviate 记忆连接">
                <t-select v-model="form.weaviate_memory_connection_id" placeholder="不使用" clearable>
                  <t-option value="" label="不使用" />
                  <t-option v-for="item in memoryWeaviateConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="false" label="Elasticsearch 图片检索连接">
                <t-select v-model="form.elasticsearch_image_connection_id" placeholder="不使用" clearable>
                  <t-option value="" label="不使用" />
                  <t-option v-for="item in imageElasticsearchConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
              <t-form-item v-if="false" label="Elasticsearch 记忆连接">
                <t-select v-model="form.elasticsearch_memory_connection_id" placeholder="不使用" clearable>
                  <t-option value="" label="不使用" />
                  <t-option v-for="item in memoryElasticsearchConnections" :key="item.config_id" :value="item.config_id" :label="item.name" />
                </t-select>
              </t-form-item>
            </div>
          </t-card>

          <t-card class="agent-service-form-section" :bordered="false">
            <template #title>Prompt engineering</template>
            <div class="agent-service-form-grid">
              <t-form-item label="System Prompt" class="agent-service-form-item-full">
                <t-textarea v-model="form.system_prompt" placeholder="可选。会追加在 QQ Chat RoleService 的通用系统规则后面。" />
              </t-form-item>
              <t-form-item label="自然语言回复 Prompt" class="agent-service-form-item-full">
                <t-textarea v-model="form.natural_language_reply_system_prompt" placeholder="可选。专门给自然语言回复模型使用的系统提示词。" />
              </t-form-item>
            </div>
          </t-card>

          <t-card class="agent-service-form-section" :bordered="false">
            <template #title>行为控制</template>
            <div class="agent-service-form-grid">
              <t-form-item label="最长输出消息长度">
                <t-input-number v-model="form.max_message_length" :min="1" />
              </t-form-item>
              <t-form-item label="用户最多 Steer 次数">
                <t-input-number v-model="form.max_steer_count" :min="0" />
                <div class="agent-service-form-hint">当 Service 还没发出最终回复时，用户继续发消息会被视为"插嘴 / steer"。这里控制单次活跃回复流程里最多接受多少次插嘴；默认 4 次，超出的消息会被丢弃。</div>
              </t-form-item>
            </div>
            <div class="agent-service-form-grid">
              <t-form-item label="情绪维度" class="agent-service-form-item-full">
                <div class="agent-service-form-hint">Service 的情绪可以由一个或者多个维度组成，这些维度共同构成 Agent 的决策、行为和输出语言风格。</div>
                <t-button variant="text" style="margin-top: 6px; padding-left: 0" @click="openEmotionDimensionsModal">管理情绪维度</t-button>
              </t-form-item>
              <t-form-item label="Rate Limit" class="agent-service-form-item-full">
                <div class="agent-service-form-hint">按 N 天/小时/分钟限制调用次数，计数跟随用户（跨群与私聊共享），优先级：用户 &gt; 群组 &gt; 默认。</div>
                <t-button variant="text" style="margin-top: 6px; padding-left: 0" @click="openRateLimitModal">编辑 Rate Limit</t-button>
              </t-form-item>
              <t-form-item label="Ignore Rules" class="agent-service-form-item-full">
                <div class="agent-service-form-hint">命中后仅做消息存储，不回复、不进入推理流程。</div>
                <t-button variant="text" style="margin-top: 6px; padding-left: 0" :disabled="Boolean(ignoreRulesDisabledReason)" @click="openIgnoreRulesModal()">管理 Ignore Rules</t-button>
                <div v-if="ignoreRulesDisabledReason" class="agent-service-form-hint" style="margin-top: 4px">
                  <InfoCircleIcon /> {{ ignoreRulesDisabledReason }}
                </div>
              </t-form-item>
            </div>
          </t-card>
        </template>

        <!-- 默认工具 -->
        <t-card class="agent-service-form-section" :bordered="false">
          <template #title>工具和能力</template>
          <div class="agent-service-default-tools-search">
            <t-input v-model="defaultToolSearchQuery" placeholder="搜索工具" clearable />
          </div>
          <div v-if="filteredDefaultTools.length === 0" class="agent-service-empty-state">没有匹配的工具。</div>
          <t-table v-else :data="filteredDefaultTools" :columns="defaultToolColumns" :hover="true" :pagination="false" row-key="id" table-layout="fixed">
            <template #enabled="{ row }">
              <t-checkbox v-model="form.default_tools_enabled[row.id]" />
            </template>
            <template #edit="{ row }">
              <t-button variant="text" size="small" @click="openDefaultToolEditModal(row.id)">编辑</t-button>
            </template>
          </t-table>
        </t-card>

        <!-- 工具调用上限回文 -->
        <t-card v-if="form.type === 'qq_chat'" class="agent-service-form-section" :bordered="false">
          <template #title>工具调用上限回文</template>
          <t-form-item label="达到调用上限回文（可选）" class="agent-service-form-item-full">
            <t-textarea v-model="form.tool_session_limit_message" placeholder="当前工具已经达到 [{limit_scope}]的调用次数限制，但是你不要把这个直接当成自然语言输出给用户，仅当用户问及详细原因时，再告知" />
          </t-form-item>
          <div class="agent-service-form-hint">留空则使用默认提示。可用 {limit_scope} 占位符表示限制范围（会替换为"单次会话"或"用户"）。</div>
        </t-card>

        <!-- 工具配置 -->
        <t-card class="agent-service-form-section" :bordered="false">
          <template #title>
            <div class="agent-service-section-title-row">
              <span>工具配置</span>
              <t-button variant="text" @click="addTool">新增工具</t-button>
            </div>
          </template>
          <div v-if="form.tools.length === 0" class="agent-service-empty-state">还没有配置工具。</div>
          <div v-for="(tool, index) in form.tools" :key="tool.id" class="agent-service-tool-block">
            <t-card :bordered="true" style="margin-top: 12px">
              <template #title>
                <div class="agent-service-tool-header">
                  <strong>工具 {{ index + 1 }}</strong>
                  <t-button variant="text" theme="danger" size="small" @click="removeTool(index)">移除</t-button>
                </div>
              </template>
              <div class="agent-service-form-grid">
                <t-form-item label="ID">
                  <t-input v-model="tool.id" />
                </t-form-item>
                <t-form-item label="名称">
                  <t-input v-model="tool.name" />
                </t-form-item>
                <t-form-item label="描述" class="agent-service-form-item-full">
                  <t-input v-model="tool.description" />
                </t-form-item>
                <t-form-item label="运行时长">
                  <t-select v-model="tool.runDuration">
                    <t-option value="Short" label="Short（短时）" />
                    <t-option value="Long" label="Long（长时）" />
                  </t-select>
                </t-form-item>
                <t-form-item label="工具模式">
                  <t-select v-model="tool.implementation">
                    <t-option value="node_graph" label="node_graph" />
                    <t-option value="python_script" label="python_script" />
                  </t-select>
                </t-form-item>
                <t-form-item v-if="tool.implementation === 'node_graph'" label="目标类型">
                  <t-select v-model="tool.targetType" @change="handleToolTargetTypeChange(tool)">
                    <t-option value="workflow_set" label="workflow_set" />
                    <t-option value="file_path" label="file_path" />
                    <t-option value="inline_graph" label="inline_graph" />
                  </t-select>
                </t-form-item>
                <t-form-item style="align-self: end">
                  <t-checkbox v-model="tool.enabled">启用该工具</t-checkbox>
                </t-form-item>
                <t-form-item v-if="form.type === 'qq_chat' && tool.enabled" label="单次会话调用上限">
                  <t-input-number v-model="form.tool_session_call_limits[tool.name]" :min="0" placeholder="不限制" />
                  <div class="agent-service-form-hint" style="font-size: 12px">0 或留空表示不限制</div>
                </t-form-item>
                <t-form-item v-if="tool.implementation === 'node_graph' && tool.targetType === 'workflow_set'" label="Workflow Set 名称" class="agent-service-form-item-full">
                  <t-select v-model="tool.workflowName" @change="applyWorkflowSetMetadata(tool)" placeholder="请选择">
                    <t-option v-for="workflow in workflows" :key="workflow.name" :value="workflow.name" :label="workflow.display_name || workflow.name" />
                  </t-select>
                </t-form-item>
                <t-form-item v-else-if="tool.implementation === 'node_graph' && tool.targetType === 'file_path'" label="文件路径" class="agent-service-form-item-full">
                  <t-input v-model="tool.filePath" placeholder="workflow_set/demo.json" />
                </t-form-item>
                <t-form-item v-else-if="tool.implementation === 'node_graph'" label="Inline Graph JSON" class="agent-service-form-item-full">
                  <t-textarea v-model="tool.inlineGraphJson" />
                </t-form-item>
                <t-form-item v-else label="Python 脚本路径" class="agent-service-form-item-full">
                  <t-input v-model="tool.pythonScriptPath" placeholder="utils/python_tools/echo_tool.py" />
                </t-form-item>
                <t-form-item v-if="tool.implementation === 'python_script'" label="入口函数名">
                  <t-input v-model="tool.pythonModuleEntry" placeholder="run_tool" />
                </t-form-item>
                <t-form-item v-if="tool.implementation === 'python_script'" label="Python 运行时">
                  <t-select v-model="tool.pythonMode">
                    <t-option value="inherit" label="继承全局设置" />
                    <t-option value="uv_project" label="uv_project" />
                    <t-option value="project_venv" label="project_venv" />
                    <t-option value="custom_executable" label="custom_executable" />
                  </t-select>
                </t-form-item>
                <t-form-item v-if="tool.implementation === 'python_script' && tool.pythonMode === 'custom_executable'" label="自定义 Python 路径" class="agent-service-form-item-full">
                  <t-input v-model="tool.pythonExecutablePath" placeholder="C:\\Python311\\python.exe" />
                </t-form-item>
                <t-form-item v-if="tool.implementation === 'python_script'" label="超时（秒）">
                  <t-input-number v-model="tool.pythonTimeoutSecs" :min="1" />
                </t-form-item>
                <t-form-item label="Parameters JSON" class="agent-service-form-item-full">
                  <template #label>
                    <div class="agent-service-params-label">
                      <span>Parameters JSON</span>
                      <t-button v-if="tool.implementation === 'node_graph' && tool.targetType === 'workflow_set' && tool.workflowName" variant="text" size="small" :disabled="syncingToolIndex === index" @click="syncToolFromGraph(tool, index)">
                        {{ syncingToolIndex === index ? '同步中…' : '从节点图更新' }}
                      </t-button>
                    </div>
                  </template>
                  <t-textarea v-model="tool.parametersJson" />
                </t-form-item>
                <t-form-item label="Outputs JSON" class="agent-service-form-item-full">
                  <t-textarea v-model="tool.outputsJson" />
                </t-form-item>
              </div>
            </t-card>
          </div>
        </t-card>
      </t-form>

      <template #footer>
        <div class="agent-service-drawer-footer">
          <t-button variant="outline" @click="closeEditModal">取消</t-button>
          <t-button theme="primary" @click="submitForm">保存</t-button>
        </div>
      </template>
    </t-drawer>

    <!-- 默认工具编辑抽屉 -->
    <t-drawer
      v-model:visible="showDefaultToolEditModal"
      :header="'编辑默认工具'"
      size="480px"
      :close-on-overlay-click="false"
      @close="closeDefaultToolEditModal"
    >
      <t-form class="agent-service-form" label-align="top">
        <t-card class="agent-service-form-section" :bordered="false">
          <t-form-item label="工具">
            <div class="agent-service-form-hint">{{ currentEditingDefaultTool?.label }} ({{ currentEditingDefaultTool?.id }})</div>
          </t-form-item>
          <t-checkbox v-model="defaultToolEditDraft.enabled">启用该工具</t-checkbox>
          <t-form-item label="单次会话调用上限" style="margin-top: 16px">
            <t-input-number v-model="defaultToolEditDraft.callLimit" :min="0" placeholder="不限制" />
            <div class="agent-service-form-hint" style="font-size: 12px; margin-top: 4px">0 或留空表示不限制</div>
          </t-form-item>
          <t-form-item v-if="editingDefaultToolId === 'image_understand'" label="图片理解模型" class="agent-service-form-item-full">
            <t-select v-model="defaultToolEditDraft.imageUnderstandLlmRefId" placeholder="默认使用主模型" clearable>
              <t-option value="" label="默认使用主模型" />
              <t-option v-for="item in multimodalChatModels" :key="item.config_id" :value="item.config_id" :label="item.name" />
            </t-select>
            <div class="agent-service-form-hint" style="margin-top: 4px">image_understand 默认使用 Service 主模型；这里只有支持多模态的模型可选。</div>
            <div v-if="form.llm_ref_id && !mainChatModelSupportsMultimodal && !defaultToolEditDraft.imageUnderstandLlmRefId" class="agent-service-form-hint" style="margin-top: 4px; color: #ffb36b">当前主模型不支持多模态，启用 image_understand 时必须在这里指定一个支持多模态的模型。</div>
          </t-form-item>
        </t-card>
      </t-form>
      <template #footer>
        <div class="agent-service-drawer-footer">
          <t-button variant="outline" @click="closeDefaultToolEditModal">取消</t-button>
          <t-button theme="primary" @click="confirmDefaultToolEdit">保存</t-button>
        </div>
      </template>
    </t-drawer>

    <!-- 情绪维度抽屉 -->
    <t-drawer
      v-model:visible="showEmotionDimensionsModal"
      header="情绪维度"
      size="640px"
      :close-on-overlay-click="false"
      :footer="false"
      @close="closeEmotionDimensionsModal"
    >
      <t-card class="agent-service-form-section" :bordered="false">
        <template #title>
          <div class="agent-service-section-title-row">
            <span>维度列表</span>
            <t-button variant="text" :disabled="emotionDimensionAdding" @click="startAddEmotionDimension">新增维度</t-button>
          </div>
        </template>

        <!-- 新增中的内联编辑卡片 -->
        <t-card v-if="emotionDimensionAdding" :bordered="true" style="margin-top: 12px">
          <template #title>
            <div class="agent-service-tool-header">
              <strong>新维度</strong>
              <t-button variant="text" @click="cancelAddEmotionDimension">取消</t-button>
            </div>
          </template>
          <div class="agent-service-form-grid">
            <t-form-item label="名称">
              <t-input v-model="emotionDimensionDraft.name" placeholder="例如：开心" />
            </t-form-item>
            <t-form-item label="升权重 (0–20)">
              <t-slider v-model="emotionDimensionDraft.increase_weight" :min="0" :max="20" :step="0.1" />
            </t-form-item>
            <t-form-item label="降权重 (0–20)">
              <t-slider v-model="emotionDimensionDraft.decrease_weight" :min="0" :max="20" :step="0.1" />
            </t-form-item>
            <t-form-item label="消解时间（小时）">
              <t-input-number v-model="emotionDimensionDraft.dissipation_hours" :min="1" :step="1" />
            </t-form-item>
            <t-form-item label="正向风格提示词（可选）" class="agent-service-form-item-full">
              <t-input v-model="emotionDimensionDraft.positive_prompt" placeholder="维度值正向时的语言风格，留空用维度名称" />
            </t-form-item>
            <t-form-item label="负向风格提示词（可选）" class="agent-service-form-item-full">
              <t-input v-model="emotionDimensionDraft.negative_prompt" placeholder="维度值负向时的语言风格，留空用「不+维度名称」" />
            </t-form-item>
          </div>
          <div class="agent-service-tool-actions" style="margin-top: 12px">
            <t-button variant="outline" @click="cancelAddEmotionDimension">取消</t-button>
            <t-button theme="primary" @click="confirmAddEmotionDimension">新增</t-button>
          </div>
        </t-card>

        <div v-if="!emotionDimensionAdding && form.emotion_dimensions.length === 0" class="agent-service-empty-state">还没有配置情绪维度。点击「新增维度」开始添加。</div>

        <!-- 已有维度卡片 -->
        <t-card v-for="(dimension, index) in form.emotion_dimensions" :key="`${dimension.name}-${index}`" :bordered="true" style="margin-top: 12px">
          <template #title>
            <div v-if="emotionDimensionEditingIndex === index" class="agent-service-tool-header">
              <strong>编辑维度</strong>
              <t-button variant="text" @click="cancelEditEmotionDimension">取消</t-button>
            </div>
            <div v-else class="agent-service-tool-header">
              <strong>{{ dimension.name }}</strong>
              <div>
                <t-button variant="text" size="small" :disabled="emotionDimensionAdding || emotionDimensionEditingIndex != null" @click="editEmotionDimension(index)">编辑</t-button>
                <t-button variant="text" theme="danger" size="small" :disabled="emotionDimensionAdding || emotionDimensionEditingIndex != null" @click="removeEmotionDimension(index)">删除</t-button>
              </div>
            </div>
          </template>

          <!-- 编辑态 -->
          <template v-if="emotionDimensionEditingIndex === index">
            <div class="agent-service-form-grid">
              <t-form-item label="名称">
                <t-input v-model="emotionDimensionDraft.name" placeholder="例如：开心" />
              </t-form-item>
              <t-form-item label="升权重 (0–20)">
                <t-slider v-model="emotionDimensionDraft.increase_weight" :min="0" :max="20" :step="0.1" />
              </t-form-item>
              <t-form-item label="降权重 (0–20)">
                <t-slider v-model="emotionDimensionDraft.decrease_weight" :min="0" :max="20" :step="0.1" />
              </t-form-item>
              <t-form-item label="消解时间（小时）">
                <t-input-number v-model="emotionDimensionDraft.dissipation_hours" :min="1" :step="1" />
              </t-form-item>
              <t-form-item label="正向风格提示词（可选）" class="agent-service-form-item-full">
                <t-input v-model="emotionDimensionDraft.positive_prompt" placeholder="维度值正向时的语言风格提示，留空用维度名称" />
              </t-form-item>
              <t-form-item label="负向风格提示词（可选）" class="agent-service-form-item-full">
                <t-input v-model="emotionDimensionDraft.negative_prompt" placeholder="维度值负向时的语言风格提示，留空用「不+维度名称」" />
              </t-form-item>
            </div>
            <div class="agent-service-tool-actions" style="margin-top: 12px">
              <t-button variant="outline" @click="cancelEditEmotionDimension">取消</t-button>
              <t-button theme="primary" @click="confirmEditEmotionDimension">保存</t-button>
            </div>
          </template>

          <!-- 展示态 -->
          <template v-else>
            <div class="agent-service-emotion-bars">
              <div class="agent-service-emotion-bar-row">
                <span class="agent-service-emotion-bar-label">升权重</span>
                <div class="agent-service-emotion-bar-track">
                  <div class="agent-service-emotion-bar-fill agent-service-emotion-bar-fill--increase" :style="{ width: Math.min(((dimension.increase_weight ?? 1) / 20) * 100, 100) + '%' }" />
                </div>
                <span class="agent-service-emotion-bar-value">{{ dimension.increase_weight ?? 1 }}</span>
              </div>
              <div class="agent-service-emotion-bar-row">
                <span class="agent-service-emotion-bar-label">降权重</span>
                <div class="agent-service-emotion-bar-track">
                  <div class="agent-service-emotion-bar-fill agent-service-emotion-bar-fill--decrease" :style="{ width: Math.min(((dimension.decrease_weight ?? 1) / 20) * 100, 100) + '%' }" />
                </div>
                <span class="agent-service-emotion-bar-value">{{ dimension.decrease_weight ?? 1 }}</span>
              </div>
            </div>
            <div class="agent-service-form-hint" style="margin-top: 8px">无对话 {{ dimension.dissipation_hours ?? 5 }} 小时后自动恢复默认</div>
            <div v-if="dimension.positive_prompt || dimension.negative_prompt" style="margin-top: 8px">
              <div v-if="dimension.positive_prompt" class="agent-service-emotion-prompt-line">
                <span class="agent-service-emotion-prompt-label">正向</span>
                <span class="agent-service-emotion-prompt-text">{{ dimension.positive_prompt }}</span>
              </div>
              <div v-if="dimension.negative_prompt" class="agent-service-emotion-prompt-line">
                <span class="agent-service-emotion-prompt-label">负向</span>
                <span class="agent-service-emotion-prompt-text">{{ dimension.negative_prompt }}</span>
              </div>
            </div>
          </template>
        </t-card>
      </t-card>
    </t-drawer>

    <!-- Ignore Rules 抽屉 -->
    <t-drawer
      v-model:visible="showIgnoreRulesModal"
      header="Ignore Rules"
      size="760px"
      :close-on-overlay-click="false"
      :footer="false"
      @close="closeIgnoreRulesModal"
    >
      <t-card class="agent-service-form-section" :bordered="false">
        <template #title>{{ ignoreRuleForm.id == null ? '新增规则' : '编辑规则' }}</template>
        <div class="agent-service-form-grid">
          <t-form-item label="sender_id">
            <t-input v-model="ignoreRuleForm.sender_id" :disabled="ignoreRuleSubmitting" placeholder="可空" />
          </t-form-item>
          <t-form-item label="group_id">
            <t-input v-model="ignoreRuleForm.group_id" :disabled="ignoreRuleSubmitting" placeholder="可空" />
          </t-form-item>
          <t-form-item label="规则说明" class="agent-service-form-item-full">
            <div class="agent-service-form-hint">{{ ignoreRulePreview }}</div>
          </t-form-item>
        </div>
        <div v-if="ignoreRuleError" class="agent-service-form-hint" style="color: var(--td-error-color); margin-top: 12px">{{ ignoreRuleError }}</div>
        <div style="display: flex; gap: 8px; margin-top: 12px">
          <t-button variant="outline" :disabled="ignoreRuleSubmitting" @click="resetIgnoreRuleForm">清空</t-button>
          <t-button theme="primary" :disabled="ignoreRuleSubmitting" @click="submitIgnoreRule">
            {{ ignoreRuleSubmitting ? (ignoreRuleForm.id == null ? '新增中…' : '保存中…') : (ignoreRuleForm.id == null ? '新增' : '保存') }}
          </t-button>
        </div>
      </t-card>

      <t-card class="agent-service-form-section" :bordered="false" style="margin-top: 16px">
        <template #title>现有规则</template>
        <div v-if="ignoreRulesLoading" class="agent-service-empty-state">加载中...</div>
        <div v-else-if="ignoreRules.length === 0" class="agent-service-empty-state">还没有规则。</div>
        <t-card v-for="rule in ignoreRules" :key="rule.id" :bordered="true" style="margin-top: 12px">
          <template #title>
            <div class="agent-service-tool-header">
              <strong>#{{ rule.id }}</strong>
              <div>
                <t-button variant="text" size="small" :disabled="ignoreRuleSubmitting || ignoreRuleDeletingId === rule.id" @click="editIgnoreRule(rule)">编辑</t-button>
                <t-button variant="text" theme="danger" size="small" :disabled="ignoreRuleSubmitting || ignoreRuleDeletingId === rule.id" @click="removeIgnoreRule(rule.id)">
                  {{ ignoreRuleDeletingId === rule.id ? '删除中…' : '删除' }}
                </t-button>
              </div>
            </div>
          </template>
          <div class="agent-service-key-value"><strong>sender_id</strong><span>{{ rule.sender_id || '未设置' }}</span></div>
          <div class="agent-service-key-value"><strong>group_id</strong><span>{{ rule.group_id || '未设置' }}</span></div>
          <div class="agent-service-key-value"><strong>含义</strong><span>{{ formatIgnoreRule(rule.sender_id, rule.group_id) }}</span></div>
        </t-card>
      </t-card>
    </t-drawer>

    <!-- Rate Limit 抽屉 -->
    <t-drawer
      v-model:visible="showRateLimitModal"
      header="Rate Limit"
      size="820px"
      :close-on-overlay-click="false"
      @close="closeRateLimitModal"
    >
      <div class="agent-service-form-hint">调用频率限制，优先级：用户 &gt; 群组 &gt; 默认。窗口可按 N 分钟 / N 小时 / N 天，计数跟随用户（跨群与私聊共享）。</div>

      <!-- 默认规则 -->
      <t-card class="agent-service-form-section" :bordered="false" style="margin-top: 12px">
        <template #title>
          <div class="agent-service-section-title-row">
            <span>默认规则</span>
            <t-checkbox v-model="form.message_rate_limit_default_enabled">启用</t-checkbox>
          </div>
        </template>
        <div v-if="form.message_rate_limit_default_enabled" class="agent-service-form-grid">
          <t-form-item label="模式">
            <t-select v-model="form.message_rate_limit_default.unlimited">
              <t-option :value="false" label="限次" />
              <t-option :value="true" label="无限" />
            </t-select>
          </t-form-item>
          <template v-if="!form.message_rate_limit_default.unlimited">
            <t-form-item label="窗口">
              <div style="display: flex; gap: 6px">
                <t-input-number v-model="form.message_rate_limit_default.window_size" :min="1" style="width: 100px" />
                <t-select v-model="form.message_rate_limit_default.window_unit" style="width: 100px">
                  <t-option value="minute" label="分钟" />
                  <t-option value="hour" label="小时" />
                  <t-option value="day" label="天" />
                </t-select>
              </div>
            </t-form-item>
            <t-form-item label="次数">
              <t-input-number v-model="form.message_rate_limit_default.max_calls" :min="1" />
            </t-form-item>
          </template>
        </div>
      </t-card>

      <!-- 群组规则 -->
      <t-card class="agent-service-form-section" :bordered="false" style="margin-top: 12px">
        <template #title>
          <div class="agent-service-section-title-row">
            <span>群组规则</span>
            <t-button variant="text" @click="addGroupRateLimitRule">新增群组规则</t-button>
          </div>
        </template>
        <div v-if="form.message_rate_limit_groups.length === 0" class="agent-service-empty-state">还没有群组规则。</div>
        <t-card v-for="(rule, index) in form.message_rate_limit_groups" :key="`group-${index}`" :bordered="true" style="margin-top: 12px">
          <template #title>
            <div class="agent-service-tool-header">
              <strong>群组规则 {{ index + 1 }}</strong>
              <t-button variant="text" theme="danger" size="small" @click="removeGroupRateLimitRule(index)">移除</t-button>
            </div>
          </template>
          <div class="agent-service-form-grid">
            <t-form-item label="Group ID">
              <t-input v-model="rule.group_id" />
            </t-form-item>
            <t-form-item label="模式">
              <t-select v-model="rule.unlimited">
                <t-option :value="false" label="限次" />
                <t-option :value="true" label="无限" />
              </t-select>
            </t-form-item>
            <template v-if="!rule.unlimited">
              <t-form-item label="窗口">
                <div style="display: flex; gap: 6px">
                  <t-input-number v-model="rule.window_size" :min="1" style="width: 100px" />
                  <t-select v-model="rule.window_unit" style="width: 100px">
                    <t-option value="minute" label="分钟" />
                    <t-option value="hour" label="小时" />
                    <t-option value="day" label="天" />
                  </t-select>
                </div>
              </t-form-item>
              <t-form-item label="次数">
                <t-input-number v-model="rule.max_calls" :min="1" />
              </t-form-item>
            </template>
          </div>
        </t-card>
      </t-card>

      <!-- 用户规则 -->
      <t-card class="agent-service-form-section" :bordered="false" style="margin-top: 12px">
        <template #title>
          <div class="agent-service-section-title-row">
            <span>用户规则</span>
            <t-button variant="text" @click="addUserRateLimitRule">新增用户规则</t-button>
          </div>
        </template>
        <div v-if="form.message_rate_limit_users.length === 0" class="agent-service-empty-state">还没有用户规则。</div>
        <t-card v-for="(rule, index) in form.message_rate_limit_users" :key="`user-${index}`" :bordered="true" style="margin-top: 12px">
          <template #title>
            <div class="agent-service-tool-header">
              <strong>用户规则 {{ index + 1 }}</strong>
              <t-button variant="text" theme="danger" size="small" @click="removeUserRateLimitRule(index)">移除</t-button>
            </div>
          </template>
          <div class="agent-service-form-grid">
            <t-form-item label="Sender ID">
              <t-input v-model="rule.sender_id" />
            </t-form-item>
            <t-form-item label="模式">
              <t-select v-model="rule.unlimited">
                <t-option :value="false" label="限次" />
                <t-option :value="true" label="无限" />
              </t-select>
            </t-form-item>
            <template v-if="!rule.unlimited">
              <t-form-item label="窗口">
                <div style="display: flex; gap: 6px">
                  <t-input-number v-model="rule.window_size" :min="1" style="width: 100px" />
                  <t-select v-model="rule.window_unit" style="width: 100px">
                    <t-option value="minute" label="分钟" />
                    <t-option value="hour" label="小时" />
                    <t-option value="day" label="天" />
                  </t-select>
                </div>
              </t-form-item>
              <t-form-item label="次数">
                <t-input-number v-model="rule.max_calls" :min="1" />
              </t-form-item>
            </template>
          </div>
        </t-card>
      </t-card>

      <template #footer>
        <div class="agent-service-drawer-footer">
          <t-button theme="primary" @click="closeRateLimitModal">完成</t-button>
        </div>
      </template>
    </t-drawer>

    <!-- Service 列表 -->
    <t-card class="agent-service-card" bordered>
      <div class="agent-service-toolbar">
        <t-input v-model="filters.keyword" clearable placeholder="搜索名称或 Config ID" />
        <t-select v-model="filters.type">
          <t-option value="all" label="全部 Service 类型" />
          <t-option value="qq_chat" label="QQ Chat RoleService" />
          <t-option value="workspace" label="Workspace RoleService" />
        </t-select>
        <t-select v-model="filters.status">
          <t-option value="all" label="全部运行状态" />
          <t-option value="running" label="运行中" />
          <t-option value="stopped" label="已停止" />
          <t-option value="error" label="异常" />
        </t-select>
        <div class="agent-service-toolbar-actions">
          <t-button variant="text" :loading="servicesLoading" @click="load">刷新</t-button>
          <span>共 {{ filteredServices.length }} 条</span>
        </div>
      </div>

      <t-table row-key="config_id" :data="filteredServices" :columns="columns" :loading="servicesLoading" :hover="true" :pagination="false" table-layout="fixed">
        <template #name="{ row }">
          <div class="agent-service-name-cell">
            <img v-if="agentAvatarUrl(row)" :src="agentAvatarUrl(row)" class="agent-service-avatar" alt="" />
            <span v-else class="agent-service-avatar agent-service-avatar--fallback">{{ agentInitial(row.name) }}</span>
            <div><strong>{{ row.name }}</strong><small class="mono">{{ compactId(row.config_id) }}</small></div>
          </div>
        </template>
        <template #type="{ row }"><t-tag variant="light">{{ serviceTypeLabel(row.role_service_type.type) }}</t-tag></template>
        <template #model="{ row }"><span :title="llmName(row)">{{ llmName(row) }}</span></template>
        <template #runtime="{ row }"><t-tag :theme="runtimeTheme(row.runtime.status)" variant="light">{{ runtimeBadgeText(row) }}</t-tag></template>
        <template #enabled="{ row }"><t-tag :theme="row.enabled ? 'success' : 'default'" variant="light">{{ row.enabled ? '已启用' : '已停用' }}</t-tag></template>
        <template #updated="{ row }"><span>{{ formatTime(row.runtime.started_at) }}</span></template>
        <template #actions="{ row }">
          <div class="agent-service-actions">
            <t-button variant="text" size="small" @click="editService(row)">编辑</t-button>
            <t-button variant="text" size="small" @click="duplicateService(row)">复制添加</t-button>
            <t-button variant="text" size="small" @click="copyServiceConfigItem(row)">{{ serviceCopiedId === row.config_id ? '已复制' : '复制' }}</t-button>
            <t-button variant="text" :theme="row.runtime.status === 'running' ? 'warning' : 'primary'" size="small" @click="toggleServiceRuntime(row)">{{ row.runtime.status === 'running' ? '停止' : '启动' }}</t-button>
            <t-popconfirm content="确认删除这个 Service 吗？" @confirm="removeService(row.config_id)"><t-button variant="text" theme="danger" size="small">删除</t-button></t-popconfirm>
          </div>
        </template>
        <template #empty><div class="agent-service-empty">暂无匹配的 Service。</div></template>
      </t-table>
    </t-card>
    <t-dialog
      v-model:visible="showModelConfigDialog"
      header="新增模型配置"
      :confirm-btn="null"
      cancel-btn="取消"
      :close-on-overlay-click="false"
    >
      <div class="agent-service-model-config-actions">
        <t-button block theme="primary" @click="openModelCreatePage">新增模型配置</t-button>
        <t-button block variant="outline" :loading="modelImporting" @click="importModelFromClipboard">从剪贴板导入</t-button>
        <t-button block variant="outline" :loading="modelImporting" @click="triggerModelImportFile">从 JSON 导入</t-button>
        <input ref="modelImportFileInput" type="file" accept=".json,application/json" class="agent-service-import-input" @change="handleModelFileChange" />
      </div>
    </t-dialog>
    <t-dialog
      v-model:visible="showRetrievalDatabaseDialog"
      header="新增检索数据库"
      :confirm-btn="null"
      cancel-btn="取消"
      :close-on-overlay-click="false"
    >
      <div class="agent-service-model-config-actions">
        <t-button block theme="primary" @click="openRetrievalDatabaseCreatePage">新增检索数据库</t-button>
        <t-button block variant="outline" :loading="retrievalDatabaseImporting" @click="importRetrievalDatabaseFromClipboard">从剪贴板导入</t-button>
        <t-button block variant="outline" :loading="retrievalDatabaseImporting" @click="triggerRetrievalDatabaseImportFile">从 JSON 导入</t-button>
        <input ref="retrievalDatabaseImportFileInput" type="file" accept=".json,application/json" class="agent-service-import-input" @change="handleRetrievalDatabaseFileChange" />
      </div>
    </t-dialog>
    <t-dialog
      v-model:visible="showWebSearchDialog"
      header="新增 Web Search"
      :confirm-btn="null"
      cancel-btn="取消"
      :close-on-overlay-click="false"
    >
      <div class="agent-service-model-config-actions">
        <t-button block theme="primary" @click="openWebSearchCreatePage">新增 Web Search</t-button>
        <t-button block variant="outline" :loading="webSearchImporting" @click="importWebSearchFromClipboard">从剪贴板导入</t-button>
        <t-button block variant="outline" :loading="webSearchImporting" @click="triggerWebSearchImportFile">从 JSON 导入</t-button>
        <input ref="webSearchImportFileInput" type="file" accept=".json,application/json" class="agent-service-import-input" @change="handleWebSearchFileChange" />
      </div>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { AddIcon, CloseIcon, InfoCircleIcon } from "tdesign-icons-vue-next";
import { system, type ServiceWithRuntime } from "../../api/client";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useAgents } from "../composables/useAgents";
import { assertConnectionConfig, assertLlmConfig } from "../model";

const {
  serviceTypes,
  services,
  servicesLoading,
  connections,
  llm,
  workflows,
  form,
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
  imageElasticsearchConnections,
  memoryElasticsearchConnections,
  retrievalConnections,
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
  duplicateService,
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
  serviceCopiedId,
  copyServiceConfig,
  handleServiceFileChange,
} = useAgents();

const router = useRouter();
const showModelConfigDialog = ref(false);
const modelImportFileInput = ref<HTMLInputElement | null>(null);
const modelImporting = ref(false);
const showRetrievalDatabaseDialog = ref(false);
const retrievalDatabaseImportFileInput = ref<HTMLInputElement | null>(null);
const retrievalDatabaseImporting = ref(false);
const showWebSearchDialog = ref(false);
const webSearchImportFileInput = ref<HTMLInputElement | null>(null);
const webSearchImporting = ref(false);

function handlePrimaryModelChange(value: string | number) {
  if (String(value) !== "__add_model__") return;
  form.llm_ref_id = "";
  showModelConfigDialog.value = true;
}

function handleMemoryBackendChange(value: string | number) {
  if (String(value) !== "__add_retrieval_database__") return;
  form.workspace_memory_backend = "";
  showRetrievalDatabaseDialog.value = true;
}

function handleWebSearchChange(value: string | number) {
  if (String(value) !== "__add_web_search__") return;
  form.web_search_engine_connection_id = "";
  showWebSearchDialog.value = true;
}

function openModelCreatePage() {
  showModelConfigDialog.value = false;
  router.push({ path: "/llm", query: { action: "create" } });
}

function openRetrievalDatabaseCreatePage() {
  showRetrievalDatabaseDialog.value = false;
  router.push({ path: "/connections", query: { action: "create" } });
}

function openWebSearchCreatePage() {
  showWebSearchDialog.value = false;
  router.push({ path: "/connections", query: { action: "create", type: "web_search_engine" } });
}

async function importModelFromText(raw: string) {
  if (modelImporting.value) return;
  modelImporting.value = true;
  try {
    const config = assertLlmConfig(JSON.parse(raw));
    const created = await system.llm.create({ name: config.name, enabled: config.enabled, model: config.model });
    await load();
    form.llm_ref_id = created.config_id;
    showModelConfigDialog.value = false;
  } catch (error) {
    alert(`模型配置导入失败：${error instanceof Error ? error.message : String(error)}`);
  } finally {
    modelImporting.value = false;
  }
}

async function importModelFromClipboard() {
  try {
    await importModelFromText(await navigator.clipboard.readText());
  } catch (error) {
    alert(`读取剪贴板失败：${error instanceof Error ? error.message : String(error)}`);
  }
}

function triggerModelImportFile() { modelImportFileInput.value?.click(); }
function handleModelFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => { void importModelFromText(String(reader.result)); input.value = ""; };
  reader.onerror = () => { alert("文件读取失败"); input.value = ""; };
  reader.readAsText(file);
}

async function importRetrievalDatabaseFromText(raw: string) {
  if (retrievalDatabaseImporting.value) return;
  retrievalDatabaseImporting.value = true;
  try {
    const config = assertConnectionConfig(JSON.parse(raw));
    const type = String(config.kind.type);
    if (type !== "weaviate" && type !== "elasticsearch") {
      throw new Error("检索数据库仅支持 Weaviate 或 Elasticsearch 连接配置");
    }
    const created = await system.connections.create({ name: config.name, enabled: config.enabled, kind: config.kind });
    await load();
    form.workspace_memory_backend = type;
    if (type === "weaviate") form.workspace_weaviate_memory_connection_id = created.config_id;
    else form.workspace_elasticsearch_memory_connection_id = created.config_id;
    showRetrievalDatabaseDialog.value = false;
  } catch (error) {
    alert(`检索数据库导入失败：${error instanceof Error ? error.message : String(error)}`);
  } finally {
    retrievalDatabaseImporting.value = false;
  }
}

async function importRetrievalDatabaseFromClipboard() {
  try {
    await importRetrievalDatabaseFromText(await navigator.clipboard.readText());
  } catch (error) {
    alert(`读取剪贴板失败：${error instanceof Error ? error.message : String(error)}`);
  }
}

function triggerRetrievalDatabaseImportFile() { retrievalDatabaseImportFileInput.value?.click(); }
function handleRetrievalDatabaseFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => { void importRetrievalDatabaseFromText(String(reader.result)); input.value = ""; };
  reader.onerror = () => { alert("文件读取失败"); input.value = ""; };
  reader.readAsText(file);
}

async function importWebSearchFromText(raw: string) {
  if (webSearchImporting.value) return;
  webSearchImporting.value = true;
  try {
    const config = assertConnectionConfig(JSON.parse(raw));
    if (config.kind.type !== "web_search_engine") {
      throw new Error("Web Search 仅支持 Web Search Engine 连接配置");
    }
    const created = await system.connections.create({ name: config.name, enabled: config.enabled, kind: config.kind });
    await load();
    form.web_search_engine_connection_id = created.config_id;
    showWebSearchDialog.value = false;
  } catch (error) {
    alert(`Web Search 导入失败：${error instanceof Error ? error.message : String(error)}`);
  } finally {
    webSearchImporting.value = false;
  }
}

async function importWebSearchFromClipboard() {
  try {
    await importWebSearchFromText(await navigator.clipboard.readText());
  } catch (error) {
    alert(`读取剪贴板失败：${error instanceof Error ? error.message : String(error)}`);
  }
}

function triggerWebSearchImportFile() { webSearchImportFileInput.value?.click(); }
function handleWebSearchFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => { void importWebSearchFromText(String(reader.result)); input.value = ""; };
  reader.onerror = () => { alert("文件读取失败"); input.value = ""; };
  reader.readAsText(file);
}

const serviceImportFileInput = ref<HTMLInputElement | null>(null);
const filters = reactive({
  keyword: "",
  type: "all",
  status: "all",
});

const filteredServices = computed(() => {
  const keyword = filters.keyword.trim().toLowerCase();
  return services.value.filter((service) => {
    if (filters.type !== "all" && service.role_service_type.type !== filters.type) {
      return false;
    }
    if (filters.status !== "all" && service.runtime.status !== filters.status) {
      return false;
    }
    if (!keyword) {
      return true;
    }
    return `${service.name} ${service.config_id}`.toLowerCase().includes(keyword);
  });
});

const columns = [
  { colKey: "name", title: "Service 名称", width: 230 },
  { colKey: "type", title: "Service 类型", width: 185 },
  { colKey: "model", title: "默认模型", ellipsis: true },
  { colKey: "runtime", title: "运行状态", width: 150 },
  { colKey: "enabled", title: "配置状态", width: 100 },
  { colKey: "updated", title: "启动时间", width: 170 },
  { colKey: "actions", title: "操作", width: 310, fixed: "right" },
];

const defaultToolColumns = [
  { colKey: "label", title: "工具名称", width: 150 },
  { colKey: "id", title: "工具 ID", width: 160 },
  { colKey: "description", title: "说明", ellipsis: true },
  { colKey: "enabled", title: "启用", width: 70 },
  { colKey: "edit", title: "编辑", width: 70 },
];

function triggerServiceImportFile() {
  serviceImportFileInput.value?.click();
}

function serviceTypeLabel(type: string): string {
  const labels: Record<string, string> = {
    qq_chat: "QQ Chat",
    workspace: "Workspace",
  };
  return labels[type] ?? type;
}

function runtimeTheme(status: string): "success" | "warning" | "danger" | "default" {
  if (status === "running") {
    return "success";
  }
  if (status === "starting") {
    return "warning";
  }
  if (status === "error") {
    return "danger";
  }
  return "default";
}

function copyServiceConfigItem(service: ServiceWithRuntime) {
  const payload = {
    name: service.name,
    enabled: service.enabled,
    auto_start: service.auto_start,
    is_default: service.is_default,
    role_service_type: service.role_service_type,
    tools: service.tools,
    ...(service.avatar_url ? { avatar_url: service.avatar_url } : {}),
  };
  copyServiceConfig(payload, service.config_id);
}
</script>

<style scoped lang="scss">
@use "../styles/agents" as *;
@use "../styles/connections" as *;
@use "../styles/dashboard" as *;

.agent-service-page {
  gap: 0;
}

.agent-service-import-input {
  display: none;
}

.agent-service-card {
  border-radius: 0;
}

.agent-service-toolbar {
  display: grid;
  grid-template-columns: minmax(220px, 1.5fr) minmax(180px, 1fr) minmax(150px, 0.8fr) auto;
  align-items: center;
  gap: 12px;
  padding-bottom: 16px;
}

.agent-service-toolbar-actions,
.agent-service-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
}

.agent-service-toolbar-actions {
  gap: 12px;
  color: var(--admin-subtle);
  font-size: 13px;
}

.agent-service-name-cell {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.agent-service-name-cell > div {
  display: grid;
  min-width: 0;
  gap: 4px;
}

.agent-service-name-cell strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-service-name-cell small {
  color: var(--admin-subtle);
  font-size: 12px;
}

.agent-service-avatar {
  width: 32px;
  height: 32px;
  flex: 0 0 32px;
  border-radius: 4px;
  object-fit: cover;
}

.agent-service-avatar--fallback {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--td-text-color-anti);
  background: var(--td-brand-color);
  font-size: 13px;
  font-weight: 600;
}

.agent-service-empty {
  padding: 56px 0;
  color: var(--admin-subtle);
  text-align: center;
}

.agent-service-empty-state {
  padding: 24px 0;
  color: var(--td-text-color-placeholder);
  text-align: center;
}

.agent-service-drawer-body {
  padding-bottom: 80px;
}

.agent-service-create-drawer-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
}

.agent-service-drawer-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
}

.agent-service-form-section {
  margin-bottom: 16px;
}

.agent-service-form-section :deep(.t-card__title) {
  font-size: 15px;
  font-weight: 600;
}

.agent-service-form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px 24px;
}

.agent-service-form-item-full {
  grid-column: 1 / -1;
}

.agent-service-form-hint {
  color: var(--td-text-color-placeholder);
  font-size: 12px;
  line-height: 1.5;
  margin-top: 4px;
}

.agent-service-form-hint a {
  color: var(--td-brand-color);
}

.agent-service-check-row {
  display: flex;
  gap: 24px;
  margin-top: 12px;
  flex-wrap: wrap;
}

.agent-service-model-config-actions {
  display: grid;
  gap: 12px;
}

:global(.t-select-option.agent-service-add-model-option) {
  position: relative;
  margin-bottom: 6px;
  color: var(--td-brand-color);
  font-weight: 600;
}

:global(.t-select-option.agent-service-add-retrieval-option) {
  position: relative;
  margin-bottom: 6px;
  color: var(--td-brand-color);
  font-weight: 600;
}

:global(.t-select-option.agent-service-add-web-search-option) {
  position: relative;
  margin-bottom: 6px;
  color: var(--td-brand-color);
  font-weight: 600;
}

:global(.agent-service-add-model-option-content) {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

:global(.agent-service-add-model-option-content .t-icon) {
  width: 16px;
  height: 16px;
  padding: 2px;
  border-radius: 50%;
  background: var(--td-brand-color);
  color: var(--td-text-color-anti);
}

:global(.t-select-option.agent-service-add-model-option::after) {
  content: "";
  position: absolute;
  right: 8px;
  bottom: -4px;
  left: 8px;
  border-bottom: 1px solid var(--td-component-border);
}

:global(.t-select-option.agent-service-add-retrieval-option::after) {
  content: "";
  position: absolute;
  right: 8px;
  bottom: -4px;
  left: 8px;
  border-bottom: 1px solid var(--td-component-border);
}

:global(.t-select-option.agent-service-add-web-search-option::after) {
  content: "";
  position: absolute;
  right: 8px;
  bottom: -4px;
  left: 8px;
  border-bottom: 1px solid var(--td-component-border);
}

.agent-service-section-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
}

.agent-service-tool-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
}

.agent-service-tool-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.agent-service-tool-block {
  margin-top: 0;
}

.agent-service-params-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
}

.agent-service-avatar-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.agent-service-avatar-preview {
  width: 64px;
  height: 64px;
  border-radius: 6px;
  object-fit: cover;
  border: 1px solid var(--td-component-border);
}

.agent-service-avatar-placeholder {
  width: 64px;
  height: 64px;
  border-radius: 6px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--td-brand-color);
  color: var(--td-text-color-anti);
  font-size: 24px;
  font-weight: 700;
}

.agent-service-avatar-actions {
  display: flex;
  gap: 8px;
}

.agent-service-type-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
}

.agent-service-type-card {
  height: auto;
  min-height: 120px;
  padding: 24px;
  display: flex;
  align-items: flex-start;
  justify-content: flex-start;
}

.agent-service-type-card-content {
  display: grid;
  gap: 8px;
  text-align: left;
}

.agent-service-type-hint {
  font-size: 13px;
  color: var(--td-text-color-placeholder);
  font-weight: 400;
}

.agent-service-default-tools-search {
  margin-bottom: 10px;
}

.agent-service-emotion-bars {
  display: grid;
  gap: 8px;
  margin-top: 8px;
}

.agent-service-emotion-bar-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.agent-service-emotion-bar-label {
  width: 52px;
  font-size: 12px;
  color: var(--td-text-color-placeholder);
  flex-shrink: 0;
}

.agent-service-emotion-bar-track {
  flex: 1;
  height: 6px;
  background: var(--td-bg-color-component);
  border-radius: 3px;
  overflow: hidden;
}

.agent-service-emotion-bar-fill {
  height: 100%;
  border-radius: 3px;
  transition: width 0.2s;
}

.agent-service-emotion-bar-fill--increase {
  background: var(--td-success-color);
}

.agent-service-emotion-bar-fill--decrease {
  background: var(--td-warning-color);
}

.agent-service-emotion-bar-value {
  width: 28px;
  font-size: 12px;
  font-weight: 600;
  text-align: right;
  flex-shrink: 0;
}

.agent-service-emotion-prompt-line {
  display: flex;
  gap: 8px;
  font-size: 12px;
  margin-top: 4px;
}

.agent-service-emotion-prompt-label {
  color: var(--td-text-color-placeholder);
  flex-shrink: 0;
  width: 32px;
}

.agent-service-emotion-prompt-text {
  color: var(--td-text-color-secondary);
}

.agent-service-key-value {
  display: flex;
  gap: 8px;
  font-size: 13px;
  margin-top: 6px;
}

.agent-service-key-value strong {
  flex-shrink: 0;
  min-width: 80px;
  color: var(--td-text-color-placeholder);
}

@media (max-width: 840px) {
  .agent-service-toolbar {
    grid-template-columns: 1fr 1fr;
  }

  .agent-service-toolbar-actions {
    justify-content: space-between;
  }

  .agent-service-type-grid {
    grid-template-columns: 1fr;
  }

  .agent-service-form-grid {
    grid-template-columns: 1fr;
  }

}

@media (max-width: 560px) {
  .agent-service-toolbar {
    grid-template-columns: 1fr;
  }
}
</style>
