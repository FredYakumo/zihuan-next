<template>
    <section class="page">
        <div class="page-hero">
            <h2>Agent 管理</h2>
            <div class="hero-actions connection-hero-actions">
                <button class="btn primary connection-hero-add-btn" @click="startCreate">+</button>
            </div>
        </div>

        <div v-if="showCreatePicker" class="connection-picker-backdrop">
            <div class="connection-picker-dialog agent-picker-dialog" @click.stop>
                <div class="connection-picker-header">
                    <h3>{{ showCreateForm ? "新建 Agent" : "选择 Agent 类型" }}</h3>
                    <button class="btn ghost connection-card-compact-btn" @click="closeCreatePicker">
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
                                <option value="qq_chat">QQ Chat Agent</option>
                                <option value="http_stream">HTTP Stream Agent</option>
                            </select>
                        </div>

                        <div class="field-full status-row">
                            <label class="field-check"><input v-model="form.enabled" type="checkbox" />启用</label>
                            <label class="field-check"><input v-model="form.auto_start" type="checkbox" />开机自动启动</label>
                            <label class="field-check"><input v-model="form.is_default" type="checkbox" />默认
                                Agent</label>
                        </div>

                        <div class="field">
                            <label>模型配置</label>
                            <select v-model="form.llm_ref_id">
                                <option value="">请选择</option>
                                <option v-for="item in chatModels" :key="item.config_id" :value="item.config_id">{{
                                    item.name }}</option>
                            </select>
                        </div>
                        <template v-if="form.type === 'qq_chat'">
                            <div class="field">
                                <label>意图分类模型</label>
                                <select v-model="form.intent_llm_ref_id">
                                    <option value="">回退主模型</option>
                                    <option v-for="item in chatModels" :key="item.config_id" :value="item.config_id">{{
                                        item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>数学编程模型</label>
                                <select v-model="form.math_programming_llm_ref_id">
                                    <option value="">回退主模型</option>
                                    <option v-for="item in chatModels" :key="item.config_id" :value="item.config_id">{{
                                        item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>文本向量模型</label>
                                <select v-model="form.embedding_model_ref_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in embeddingModels" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>分词 Tokenizer 连接</label>
                                <select v-model="form.tokenizer_connection_id">
                                    <option value="">不使用（标点分段）</option>
                                    <option v-for="item in tokenizerConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Bot Adapter</label>
                                <select v-model="form.ims_bot_adapter_connection_id">
                                    <option value="">请选择</option>
                                    <option v-for="item in botConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field"><label>Bot Name</label><input v-model="form.bot_name" /></div>
                            <div class="field-full">
                                <label>System Prompt</label>
                                <textarea v-model="form.system_prompt" placeholder="可选。会追加在 QQ Chat Agent 的通用系统规则后面。" />
                            </div>
                            <div class="field">
                                <label>RustFS Connection</label>
                                <select v-model="form.rustfs_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in rustfsConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Web Search Engine</label>
                                <select v-model="form.web_search_engine_connection_id">
                                    <option value="">请选择</option>
                                    <option v-for="item in webSearchEngineConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>RDB Connection</label>
                                <select v-model="form.rdb_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in taskDbConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Weaviate Image Connection</label>
                                <select v-model="form.weaviate_image_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in imageWeaviateConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Weaviate Memory Connection</label>
                                <select v-model="form.weaviate_memory_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in memoryWeaviateConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field"><label>Max Message Length</label><input
                                    v-model.number="form.max_message_length" type="number" min="1" /></div>
                            <div class="field">
                                <label>Max Steer Count</label>
                                <div class="muted">
                                    当 Agent 还没发出最终回复时，用户继续发消息会被视为"插嘴 / steer"。
                                    这里控制单次活跃回复流程里最多接受多少次插嘴；默认 4 次，超出会被丢弃并写入日志。
                                </div>
                                <input v-model.number="form.max_steer_count" type="number" min="0" />
                            </div>
                            <div class="field"><label>Compact Context Length</label><input
                                    v-model.number="form.compact_context_length" type="number" min="0" /></div>
                            <div class="field">
                                <label>Ignore Rules</label>
                                <div class="muted" style="margin-top: 2px;">
                                    命中后仅做消息存储，不回复、不进入推理流程。
                                </div>
                                <button class="btn ghost" type="button" style="margin-top: 6px;"
                                    :disabled="Boolean(ignoreRulesDisabledReason)" @click="openIgnoreRulesModal()">
                                    管理 Ignore Rules
                                </button>
                                <div v-if="ignoreRulesDisabledReason" class="muted"
                                    style="margin-top: 4px; font-size: 12px;">
                                    💡 {{ ignoreRulesDisabledReason }}
                                </div>
                            </div>
                        </template>

                        <template v-else>
                            <div class="field"><label>Bind</label><input v-model="form.http_bind"
                                    placeholder="127.0.0.1:18080" /></div>
                            <div class="field"><label>API Key</label><input v-model="form.http_api_key" /></div>
                            <div class="field">
                                <label>Web Search Engine</label>
                                <select v-model="form.http_web_search_engine_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in webSearchEngineConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Task DB Connection</label>
                                <select v-model="form.task_db_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in taskDbConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                                <div v-if="!form.task_db_connection_id" class="muted" style="margin-top: 4px;">
                                    💡 未配置关系数据库连接时，任务记录仅在内存中保存，重启服务后会丢失。
                                    如需持久化，请在 <a href="#/connections" style="color: var(--primary);">连接管理</a> 中新建 MySQL 或
                                    SQLite 连接。
                                </div>
                            </div>
                            <div class="field">
                                <label>Memory Embedding Model</label>
                                <select v-model="form.http_embedding_model_ref_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in embeddingModels" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Weaviate Memory Connection</label>
                                <select v-model="form.http_weaviate_memory_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in memoryWeaviateConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                        </template>
                    </div>

                    <div v-if="form.type === 'qq_chat'" class="editor-card" style="margin-top: 12px;">
                        <div class="split-header">
                            <div>
                                <h3>默认工具</h3>
                            </div>
                        </div>
                        <div class="list" style="margin-top: 12px;">
                            <label v-for="tool in qqChatDefaultTools" :key="tool.id" class="field-check"
                                style="display: flex; align-items: flex-start; gap: 8px; margin-bottom: 8px;">
                                <input v-model="form.default_tools_enabled[tool.id]" type="checkbox" />
                                <span style="flex: 1;">
                                    <strong>{{ tool.label }}</strong>
                                    <span class="muted" style="display: block;">{{ tool.description }}</span>
                                    <div v-if="tool.id === 'image_understand' && form.default_tools_enabled.image_understand !== false"
                                        style="margin-top: 8px;">
                                        <label>图片理解模型</label>
                                        <select v-model="form.image_understand_llm_ref_id">
                                            <option value="">默认使用主模型</option>
                                            <option v-for="item in multimodalChatModels" :key="item.config_id"
                                                :value="item.config_id">{{ item.name }}</option>
                                        </select>
                                        <div class="muted" style="margin-top: 4px;">
                                            image_understand 默认使用 Agent 主模型；这里只有支持多模态的模型可选。
                                        </div>
                                        <div v-if="form.llm_ref_id && !mainChatModelSupportsMultimodal && !form.image_understand_llm_ref_id"
                                            class="muted" style="margin-top: 4px; color: #ffb36b;">
                                            当前主模型不支持多模态，启用 image_understand 时必须在这里指定一个支持多模态的模型。
                                        </div>
                                    </div>
                                </span>
                            </label>
                        </div>
                    </div>

                    <div v-else class="editor-card" style="margin-top: 12px;">
                        <div class="split-header">
                            <div>
                                <h3>默认工具</h3>
                            </div>
                        </div>
                        <div class="list" style="margin-top: 12px;">
                            <label v-for="tool in httpStreamDefaultTools" :key="tool.id" class="field-check"
                                style="display: flex; align-items: flex-start; gap: 8px; margin-bottom: 8px;">
                                <input v-model="form.default_tools_enabled[tool.id]" type="checkbox" />
                                <span>
                                    <strong>{{ tool.label }}</strong>
                                    <span class="muted" style="display: block;">{{ tool.description }}</span>
                                </span>
                            </label>
                        </div>
                    </div>

                    <div class="editor-card" style="margin-top: 18px;">
                        <div class="split-header">
                            <div>
                                <h3>工具配置</h3>
                            </div>
                            <button class="btn ghost" @click="addTool">新增工具</button>
                        </div>
                        <div class="list" style="margin-top: 14px;">
                            <div v-if="form.tools.length === 0" class="empty-state">还没有配置工具。</div>
                            <div v-for="(tool, index) in form.tools" :key="tool.id" class="tool-block">
                                <div class="split-header">
                                    <strong>工具 {{ index + 1 }}</strong>
                                    <button class="btn warn" @click="removeTool(index)">移除</button>
                                </div>
                                <div class="form-grid">
                                    <div class="field"><label>ID</label><input v-model="tool.id" /></div>
                                    <div class="field"><label>名称</label><input v-model="tool.name" /></div>
                                    <div class="field-full"><label>描述</label><input v-model="tool.description" /></div>
                                    <div class="field">
                                        <label>运行时长</label>
                                        <select v-model="tool.runDuration">
                                            <option value="Short">Short（短时）</option>
                                            <option value="Long">Long（长时）</option>
                                        </select>
                                    </div>
                                    <div class="field">
                                        <label>目标类型</label>
                                        <select v-model="tool.targetType" @change="handleToolTargetTypeChange(tool)">
                                            <option value="workflow_set">workflow_set</option>
                                            <option value="file_path">file_path</option>
                                            <option value="inline_graph">inline_graph</option>
                                        </select>
                                    </div>
                                    <div class="field field-check"><input v-model="tool.enabled" type="checkbox" />启用该工具
                                    </div>
                                    <div v-if="tool.targetType === 'workflow_set'" class="field-full">
                                        <label>Workflow Set 名称</label>
                                        <select v-model="tool.workflowName" @change="applyWorkflowSetMetadata(tool)">
                                            <option value="">请选择</option>
                                            <option v-for="workflow in workflows" :key="workflow.name"
                                                :value="workflow.name">
                                                {{ workflow.display_name || workflow.name }}
                                            </option>
                                        </select>
                                    </div>
                                    <div v-else-if="tool.targetType === 'file_path'" class="field-full">
                                        <label>文件路径</label>
                                        <input v-model="tool.filePath" placeholder="workflow_set/demo.json" />
                                    </div>
                                    <div v-else class="field-full">
                                        <label>Inline Graph JSON</label>
                                        <textarea v-model="tool.inlineGraphJson" />
                                    </div>
                                    <div class="field-full">
                                        <div
                                            style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 4px;">
                                            <label style="margin-bottom: 0;">Parameters JSON</label>
                                            <button v-if="tool.targetType === 'workflow_set' && tool.workflowName"
                                                class="btn ghost" style="padding: 2px 10px; font-size: 12px;"
                                                :disabled="syncingToolIndex === index"
                                                @click="syncToolFromGraph(tool, index)">{{ syncingToolIndex === index ?
                                                '同步中…' : '从节点图更新'
                                                }}</button>
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
                        <button class="btn ghost" @click="showCreateForm = false">返回</button>
                        <button class="btn primary" @click="submitForm">创建 Agent</button>
                    </div>
                </div>

                <div v-else class="connection-picker-grid">
                    <button v-for="type in agentTypes" :key="type.value" class="connection-picker-option"
                        @click="pickCreateType(type.value)">
                        <strong>{{ type.label }}</strong>
                        <span>{{ type.hint }}</span>
                    </button>
                </div>
            </div>
        </div>

        <!-- 编辑 Agent 模态框 -->
        <div v-if="showEditModal" class="agent-edit-modal-backdrop" @click.stop>
            <div class="agent-edit-modal" @click.stop>
                <div class="agent-edit-modal-header">
                    <div class="connection-card-badges">
                        <span class="badge">{{ form.type }}</span>
                        <span class="badge" :class="form.enabled ? 'success' : ''">{{ form.enabled ? "已启用" : "已停用"
                            }}</span>
                        <span v-if="form.is_default" class="badge">default</span>
                    </div>
                    <h3 style="margin: 0;">{{ form.name || '编辑 Agent' }}</h3>
                </div>

                <div class="agent-edit-modal-body">
                    <div class="form-grid">
                        <div class="field">
                            <label>名称</label>
                            <input v-model="form.name" />
                        </div>
                        <div class="field">
                            <label>类型</label>
                            <select v-model="form.type">
                                <option value="qq_chat">QQ Chat Agent</option>
                                <option value="http_stream">HTTP Stream Agent</option>
                            </select>
                        </div>

                        <div class="field-full status-row">
                            <label class="field-check"><input v-model="form.enabled" type="checkbox" />启用</label>
                            <label class="field-check"><input v-model="form.auto_start" type="checkbox" />开机自动启动</label>
                            <label class="field-check"><input v-model="form.is_default" type="checkbox" />默认
                                Agent</label>
                        </div>

                        <div class="field">
                            <label>模型配置</label>
                            <select v-model="form.llm_ref_id">
                                <option value="">请选择</option>
                                <option v-for="item in chatModels" :key="item.config_id" :value="item.config_id">{{
                                    item.name }}
                                </option>
                            </select>
                        </div>
                        <template v-if="form.type === 'qq_chat'">
                            <div class="field">
                                <label>意图分类模型</label>
                                <select v-model="form.intent_llm_ref_id">
                                    <option value="">回退主模型</option>
                                    <option v-for="item in chatModels" :key="item.config_id" :value="item.config_id">{{
                                        item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>数学编程模型</label>
                                <select v-model="form.math_programming_llm_ref_id">
                                    <option value="">回退主模型</option>
                                    <option v-for="item in chatModels" :key="item.config_id" :value="item.config_id">{{
                                        item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>文本向量模型</label>
                                <select v-model="form.embedding_model_ref_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in embeddingModels" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>分词 Tokenizer 连接</label>
                                <select v-model="form.tokenizer_connection_id">
                                    <option value="">不使用（标点分段）</option>
                                    <option v-for="item in tokenizerConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Bot Adapter</label>
                                <select v-model="form.ims_bot_adapter_connection_id">
                                    <option value="">请选择</option>
                                    <option v-for="item in botConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field"><label>Bot Name</label><input v-model="form.bot_name" /></div>
                            <div class="field-full">
                                <label>System Prompt</label>
                                <textarea v-model="form.system_prompt" placeholder="可选。会追加在 QQ Chat Agent 的通用系统规则后面。"
                                    style="min-height: 100px;" />
                            </div>
                            <div class="field">
                                <label>RustFS Connection</label>
                                <select v-model="form.rustfs_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in rustfsConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Web Search Engine</label>
                                <select v-model="form.web_search_engine_connection_id">
                                    <option value="">请选择</option>
                                    <option v-for="item in webSearchEngineConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>RDB Connection</label>
                                <select v-model="form.rdb_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in taskDbConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Weaviate Image Connection</label>
                                <select v-model="form.weaviate_image_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in imageWeaviateConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Weaviate Memory Connection</label>
                                <select v-model="form.weaviate_memory_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in memoryWeaviateConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field"><label>Max Message Length</label><input
                                    v-model.number="form.max_message_length" type="number" min="1" /></div>
                            <div class="field">
                                <label>Max Steer Count</label>
                                <div class="muted">
                                    当 Agent 还没发出最终回复时，用户继续发消息会被视为"插嘴 / steer"。
                                    这里控制单次活跃回复流程里最多接受多少次插嘴；默认 4 次，超出会被丢弃并写入日志。
                                </div>
                                <input v-model.number="form.max_steer_count" type="number" min="0" />
                            </div>
                            <div class="field"><label>Compact Context Length</label><input
                                    v-model.number="form.compact_context_length" type="number" min="0" /></div>
                            <div class="field">
                                <label>Ignore Rules</label>
                                <div class="muted" style="margin-top: 2px;">
                                    命中后仅做消息存储，不回复、不进入推理流程。
                                </div>
                                <button class="btn ghost" type="button" style="margin-top: 6px;"
                                    :disabled="Boolean(ignoreRulesDisabledReason)" @click="openIgnoreRulesModal()">
                                    管理 Ignore Rules
                                </button>
                                <div v-if="ignoreRulesDisabledReason" class="muted"
                                    style="margin-top: 4px; font-size: 12px;">
                                    💡 {{ ignoreRulesDisabledReason }}
                                </div>
                            </div>

                            <div class="editor-card" style="margin-top: 12px;">
                                <div class="split-header">
                                    <div>
                                        <h3>默认工具</h3>
                                    </div>
                                </div>
                        <div class="list" style="margin-top: 12px;">
                            <label v-for="tool in qqChatDefaultTools" :key="tool.id" class="field-check"
                                style="display: flex; align-items: flex-start; gap: 8px; margin-bottom: 8px;">
                                <input v-model="form.default_tools_enabled[tool.id]" type="checkbox" />
                                <span style="flex: 1;">
                                    <strong>{{ tool.label }}</strong>
                                    <span class="muted" style="display: block;">{{ tool.description }}</span>
                                    <div v-if="tool.id === 'image_understand' && form.default_tools_enabled.image_understand !== false"
                                        style="margin-top: 8px;">
                                        <label>图片理解模型</label>
                                        <select v-model="form.image_understand_llm_ref_id">
                                            <option value="">默认使用主模型</option>
                                            <option v-for="item in multimodalChatModels" :key="item.config_id"
                                                :value="item.config_id">{{ item.name }}</option>
                                        </select>
                                        <div class="muted" style="margin-top: 4px;">
                                            image_understand 默认使用 Agent 主模型；这里只有支持多模态的模型可选。
                                        </div>
                                        <div v-if="form.llm_ref_id && !mainChatModelSupportsMultimodal && !form.image_understand_llm_ref_id"
                                            class="muted" style="margin-top: 4px; color: #ffb36b;">
                                            当前主模型不支持多模态，启用 image_understand 时必须在这里指定一个支持多模态的模型。
                                        </div>
                                    </div>
                                </span>
                            </label>
                        </div>
                    </div>
                        </template>

                        <template v-else>
                            <div class="field"><label>Bind</label><input v-model="form.http_bind"
                                    placeholder="127.0.0.1:18080" /></div>
                            <div class="field"><label>API Key</label><input v-model="form.http_api_key" /></div>
                            <div class="field">
                                <label>Web Search Engine</label>
                                <select v-model="form.http_web_search_engine_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in webSearchEngineConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Task DB Connection</label>
                                <select v-model="form.task_db_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in taskDbConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                                <div v-if="!form.task_db_connection_id" class="muted" style="margin-top: 4px;">
                                    💡 未配置关系数据库连接时，任务记录仅在内存中保存，重启服务后会丢失。
                                    如需持久化，请在 <a href="#/connections" style="color: var(--primary);">连接管理</a> 中新建 MySQL 或
                                    SQLite 连接。
                                </div>
                            </div>
                            <div class="field">
                                <label>Memory Embedding Model</label>
                                <select v-model="form.http_embedding_model_ref_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in embeddingModels" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>
                            <div class="field">
                                <label>Weaviate Memory Connection</label>
                                <select v-model="form.http_weaviate_memory_connection_id">
                                    <option value="">不使用</option>
                                    <option v-for="item in memoryWeaviateConnections" :key="item.config_id"
                                        :value="item.config_id">{{ item.name }}</option>
                                </select>
                            </div>

                            <div class="editor-card" style="margin-top: 12px;">
                                <div class="split-header">
                                    <div>
                                        <h3>默认工具</h3>
                                    </div>
                                </div>
                                <div class="list" style="margin-top: 12px;">
                                    <label v-for="tool in httpStreamDefaultTools" :key="tool.id" class="field-check"
                                        style="display: flex; align-items: flex-start; gap: 8px; margin-bottom: 8px;">
                                        <input v-model="form.default_tools_enabled[tool.id]" type="checkbox" />
                                        <span>
                                            <strong>{{ tool.label }}</strong>
                                            <span class="muted" style="display: block;">{{ tool.description }}</span>
                                        </span>
                                    </label>
                                </div>
                            </div>
                        </template>
                    </div>

                    <div class="editor-card" style="margin-top: 18px;">
                        <div class="split-header">
                            <div>
                                <h3>工具配置</h3>
                            </div>
                            <button class="btn ghost" @click="addTool">新增工具</button>
                        </div>
                        <div class="list" style="margin-top: 14px;">
                            <div v-if="form.tools.length === 0" class="empty-state">还没有配置工具。</div>
                            <div v-for="(tool, index) in form.tools" :key="tool.id" class="tool-block">
                                <div class="split-header">
                                    <strong>工具 {{ index + 1 }}</strong>
                                    <button class="btn warn" @click="removeTool(index)">移除</button>
                                </div>
                                <div class="form-grid">
                                    <div class="field"><label>ID</label><input v-model="tool.id" /></div>
                                    <div class="field"><label>名称</label><input v-model="tool.name" /></div>
                                    <div class="field-full"><label>描述</label><input v-model="tool.description" /></div>
                                    <div class="field">
                                        <label>运行时长</label>
                                        <select v-model="tool.runDuration">
                                            <option value="Short">Short（短时）</option>
                                            <option value="Long">Long（长时）</option>
                                        </select>
                                    </div>
                                    <div class="field">
                                        <label>目标类型</label>
                                        <select v-model="tool.targetType" @change="handleToolTargetTypeChange(tool)">
                                            <option value="workflow_set">workflow_set</option>
                                            <option value="file_path">file_path</option>
                                            <option value="inline_graph">inline_graph</option>
                                        </select>
                                    </div>
                                    <div class="field field-check"><input v-model="tool.enabled" type="checkbox" />启用该工具
                                    </div>
                                    <div v-if="tool.targetType === 'workflow_set'" class="field-full">
                                        <label>Workflow Set 名称</label>
                                        <select v-model="tool.workflowName" @change="applyWorkflowSetMetadata(tool)">
                                            <option value="">请选择</option>
                                            <option v-for="workflow in workflows" :key="workflow.name"
                                                :value="workflow.name">
                                                {{ workflow.display_name || workflow.name }}
                                            </option>
                                        </select>
                                    </div>
                                    <div v-else-if="tool.targetType === 'file_path'" class="field-full">
                                        <label>文件路径</label>
                                        <input v-model="tool.filePath" placeholder="workflow_set/demo.json" />
                                    </div>
                                    <div v-else class="field-full">
                                        <label>Inline Graph JSON</label>
                                        <textarea v-model="tool.inlineGraphJson" />
                                    </div>
                                    <div class="field-full">
                                        <div
                                            style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 4px;">
                                            <label style="margin-bottom: 0;">Parameters JSON</label>
                                            <button v-if="tool.targetType === 'workflow_set' && tool.workflowName"
                                                class="btn ghost" style="padding: 2px 10px; font-size: 12px;"
                                                :disabled="syncingToolIndex === index"
                                                @click="syncToolFromGraph(tool, index)">{{ syncingToolIndex === index ?
                                                '同步中…' :
                                                '从节点图更新' }}</button>
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

                <div class="agent-edit-modal-footer">
                    <button class="btn ghost" @click="closeEditModal">取消</button>
                    <button class="btn primary" @click="submitForm">保存</button>
                </div>
            </div>
        </div>

        <div v-if="showIgnoreRulesModal" class="agent-edit-modal-backdrop" @click.stop>
            <div class="agent-edit-modal" @click.stop style="max-width: 760px;">
                <div class="agent-edit-modal-header">
                    <h3 style="margin: 0;">Ignore Rules</h3>
                    <button class="btn ghost" @click="closeIgnoreRulesModal">关闭</button>
                </div>
                <div class="agent-edit-modal-body">
                    <div class="editor-card">
                        <div class="split-header">
                            <div>
                                <h3>{{ ignoreRuleForm.id == null ? "新增规则" : "编辑规则" }}</h3>
                            </div>
                        </div>
                        <div class="form-grid" style="margin-top: 12px;">
                            <div class="field">
                                <label>sender_id</label>
                                <input v-model="ignoreRuleForm.sender_id" :disabled="ignoreRuleSubmitting"
                                    placeholder="可空" />
                            </div>
                            <div class="field">
                                <label>group_id</label>
                                <input v-model="ignoreRuleForm.group_id" :disabled="ignoreRuleSubmitting"
                                    placeholder="可空" />
                            </div>
                            <div class="field-full">
                                <label>规则说明</label>
                                <div class="muted">{{ ignoreRulePreview }}</div>
                            </div>
                        </div>
                        <div v-if="ignoreRuleError" class="muted"
                            style="margin-top: 12px; color: var(--danger, #d9534f);">
                            {{ ignoreRuleError }}
                        </div>
                        <div class="panel-actions" style="margin-top: 12px;">
                            <button class="btn ghost" :disabled="ignoreRuleSubmitting"
                                @click="resetIgnoreRuleForm">清空</button>
                            <button class="btn primary" :disabled="ignoreRuleSubmitting" @click="submitIgnoreRule">{{
                                ignoreRuleSubmitting ? (ignoreRuleForm.id == null ? "新增中…" : "保存中…") :
                                (ignoreRuleForm.id ==
                                null ? "新增" : "保存") }}</button>
                        </div>
                    </div>

                    <div class="editor-card" style="margin-top: 16px;">
                        <div class="split-header">
                            <div>
                                <h3>现有规则</h3>
                            </div>
                        </div>
                        <div class="list" style="margin-top: 12px;">
                            <div v-if="ignoreRulesLoading" class="empty-state">加载中...</div>
                            <div v-else-if="ignoreRules.length === 0" class="empty-state">还没有规则。</div>
                            <div v-for="rule in ignoreRules" :key="rule.id" class="tool-block">
                                <div class="split-header">
                                    <strong>#{{ rule.id }}</strong>
                                    <div class="inline-actions">
                                        <button class="btn ghost connection-card-compact-btn"
                                            :disabled="ignoreRuleSubmitting || ignoreRuleDeletingId === rule.id"
                                            @click="editIgnoreRule(rule)">编辑</button>
                                        <button class="btn warn connection-card-compact-btn"
                                            :disabled="ignoreRuleSubmitting || ignoreRuleDeletingId === rule.id"
                                            @click="removeIgnoreRule(rule.id)">{{ ignoreRuleDeletingId === rule.id ?
                                            "删除中…" :
                                            "删除" }}</button>
                                    </div>
                                </div>
                                <div class="key-value"><strong>sender_id</strong><span>{{ rule.sender_id || "未设置"
                                        }}</span>
                                </div>
                                <div class="key-value"><strong>group_id</strong><span>{{ rule.group_id || "未设置"
                                        }}</span></div>
                                <div class="key-value"><strong>含义</strong><span>{{ formatIgnoreRule(rule.sender_id,
                                        rule.group_id) }}</span></div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>

        <section v-if="agentsLoading && agents.length === 0" class="panel">
            <div class="agent-loading-state" aria-live="polite">
                <span class="agent-loading-spinner"></span>
                <span>Agent 加载中...</span>
            </div>
        </section>

        <section v-else-if="agents.length > 0" class="panel">
            <div class="connection-grid connection-grid--agents" style="margin-top: 0;">
                <article v-for="agent in agents" :key="agent.config_id" class="connection-card">
                    <div class="connection-card-header connection-card-header--stacked">
                        <div class="connection-card-header-top">
                            <div class="connection-card-badges">
                                <span class="badge">{{ agent.agent_type.type }}</span>
                                <span class="badge" :class="agent.enabled ? 'success' : ''">{{ agent.enabled ? "已启用" :
                                    "已停用"
                                    }}</span>
                                <span class="badge" :class="statusTone(agent.runtime.status)">{{ runtimeBadgeText(agent)
                                    }}</span>
                                <span v-if="agent.is_default" class="badge">default</span>
                            </div>
                            <div class="inline-actions connection-card-display-actions">
                                <button class="btn ghost connection-card-compact-btn"
                                    @click="editAgent(agent)">编辑</button>
                                <button class="btn connection-card-compact-btn" @click="toggleAgentRuntime(agent)">
                                    {{ agent.runtime.status === "running" ? "停止" : "启动" }}
                                </button>
                                <button class="btn warn connection-card-compact-btn"
                                    @click="removeAgent(agent.config_id)">删除</button>
                            </div>
                        </div>
                        <div style="display: flex; align-items: center; gap: 10px;">
                            <img v-if="botAvatarUrl(agent)" :src="botAvatarUrl(agent)" alt="bot avatar"
                                style="width: 36px; height: 36px; border-radius: 999px; border: 1px solid var(--line); object-fit: cover; background: var(--surface-soft);" />
                            <h4 style="margin: 0;">{{ agent.name }}</h4>
                        </div>
                    </div>

                    <div class="connection-card-body">
                        <div v-for="item in summarizeAgent(agent)" :key="item.label" class="key-value">
                            <strong>{{ item.label }}</strong>
                            <span :class="item.mono ? 'mono' : ''">{{ item.value }}</span>
                        </div>
                    </div>

                    <div class="connection-card-footer">
                        <span class="muted">启动于 {{ formatTime(agent.runtime.started_at) }}</span>
                        <span class="muted">工具 {{ agent.tools.length }} 个</span>
                    </div>
                </article>
            </div>
        </section>

        <section v-else class="panel">
            <div class="empty-state">当前没有 Agent。</div>
        </section>
    </section>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";

import { system, workflows as workflowApi, type AgentWithRuntime, type ConnectionConfig, type LlmConfig, type QqChatAgentIgnoreRule, type WorkflowInfo } from "../../api/client";
import {
    agentFormFromConfig,
    buildAgentPayload,
    HTTP_STREAM_DEFAULT_TOOLS,
    isBotAdapterConnectionType,
    QQ_CHAT_DEFAULT_TOOLS,
    defaultAgentForm,
    defaultToolForm,
    compactId,
    formatTime,
    statusTone,
    summarizeIds,
    type AgentFormState,
    type AgentTypeName,
} from "../model";

type AgentTypeOption = {
    value: AgentTypeName;
    label: string;
    hint: string;
};

const agentTypes: AgentTypeOption[] = [
    { value: "qq_chat", label: "QQ Chat Agent", hint: "通过 QQ Bot Adapter 提供对话服务" },
    { value: "http_stream", label: "HTTP Stream Agent", hint: "通过 HTTP 流式接口对外提供服务" },
];

const agents = ref<AgentWithRuntime[]>([]);
const agentsLoading = ref(false);
const connections = ref<ConnectionConfig[]>([]);
const llm = ref<LlmConfig[]>([]);
const workflows = ref<WorkflowInfo[]>([]);
const form = reactive<AgentFormState>(defaultAgentForm());
const editingAgentId = ref("");
const showCreatePicker = ref(false);
const showCreateForm = ref(false);
const showEditModal = ref(false);
const showIgnoreRulesModal = ref(false);
const ignoreRulesLoading = ref(false);
const ignoreRules = ref<QqChatAgentIgnoreRule[]>([]);
const ignoreRuleSubmitting = ref(false);
const ignoreRuleDeletingId = ref<number | null>(null);
const ignoreRuleError = ref("");
const ignoreRuleForm = reactive<{ id: number | null; sender_id: string; group_id: string }>({
    id: null,
    sender_id: "",
    group_id: "",
});
const qqChatDefaultTools = QQ_CHAT_DEFAULT_TOOLS;
const httpStreamDefaultTools = HTTP_STREAM_DEFAULT_TOOLS;
const chatModels = computed(() => llm.value.filter((item) => item.model.type === "chat_llm"));
const multimodalChatModels = computed(
    () =>
        llm.value.filter(
            (item) =>
                item.model.type === "chat_llm" && Boolean(item.model.llm.supports_multimodal_input),
        ),
);
const embeddingModels = computed(() => llm.value.filter((item) => item.model.type === "text_embedding_local" && item.enabled));
const mainChatModel = computed(() => llm.value.find((item) => item.config_id === form.llm_ref_id));
const mainChatModelSupportsMultimodal = computed(() => {
    const selected = mainChatModel.value;
    return Boolean(selected?.model.type === "chat_llm" && selected.model.llm.supports_multimodal_input);
});

const botConnections = computed(() => connections.value.filter((item) => isBotAdapterConnectionType(String(item.kind.type ?? ""))));
const rustfsConnections = computed(() => connections.value.filter((item) => item.kind.type === "rustfs"));
const webSearchEngineConnections = computed(() => connections.value.filter((item) => item.kind.type === "web_search_engine"));
const taskDbConnections = computed(() => connections.value.filter((item) => item.kind.type === "mysql" || item.kind.type === "sqlite"));
const tokenizerConnections = computed(() => connections.value.filter((item) => item.kind.type === "tokenizer"));
const imageWeaviateConnections = computed(() =>
    connections.value.filter((item) => item.kind.type === "weaviate" && item.kind.collection_schema === "image_semantic"),
);
const memoryWeaviateConnections = computed(() =>
    connections.value.filter((item) => item.kind.type === "weaviate" && item.kind.collection_schema === "agent_memory"),
);
const ignoreRulesDisabledReason = computed(() => {
    if (!editingAgentId.value) {
        return "请先保存当前 Agent，再管理 Ignore Rules。";
    }
    if (!form.rdb_id) {
        return "先配置 RDB Connection，Ignore Rules 和任务/消息持久化都会共用这条关系库连接。";
    }
    return "";
});

function resetForm() {
    Object.assign(form, defaultAgentForm());
}

function clearEditingAgent() {
    editingAgentId.value = "";
}

const ignoreRulePreview = computed(() => formatIgnoreRule(ignoreRuleForm.sender_id, ignoreRuleForm.group_id));

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
    showCreatePicker.value = false;
    showCreateForm.value = false;
}

function pickCreateType(type: AgentTypeName) {
    resetForm();
    clearEditingAgent();
    form.type = type;
    showCreatePicker.value = true;
    showCreateForm.value = true;
}

function closeEditor() {
    showCreatePicker.value = false;
    showCreateForm.value = false;
    closeEditModal();
}

async function load() {
    agentsLoading.value = true;
    try {
        const [loadedAgents, loadedConnections, loadedLlm, loadedWorkflows] = await Promise.all([
            system.agents.list(),
            system.connections.list(),
            system.llm.list(),
            workflowApi.listDetailed(),
        ]);
        agents.value = loadedAgents;
        connections.value = loadedConnections;
        llm.value = loadedLlm;
        workflows.value = loadedWorkflows.workflows;
    } finally {
        agentsLoading.value = false;
    }
}

function editAgent(agent: AgentWithRuntime) {
    Object.assign(form, agentFormFromConfig(agent));
    editingAgentId.value = agent.config_id;
    showEditModal.value = true;
}

function closeEditModal() {
    showEditModal.value = false;
    resetForm();
    clearEditingAgent();
}

function resetIgnoreRuleForm() {
    ignoreRuleForm.id = null;
    ignoreRuleForm.sender_id = "";
    ignoreRuleForm.group_id = "";
    ignoreRuleError.value = "";
}

function formatIgnoreRule(senderId: string | null | undefined, groupId: string | null | undefined): string {
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
    if (!editingAgentId.value) {
        return;
    }
    ignoreRulesLoading.value = true;
    try {
        ignoreRuleError.value = "";
        ignoreRules.value = await system.agents.listIgnoreRules(editingAgentId.value);
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

function editIgnoreRule(rule: QqChatAgentIgnoreRule) {
    ignoreRuleForm.id = rule.id;
    ignoreRuleForm.sender_id = rule.sender_id ?? "";
    ignoreRuleForm.group_id = rule.group_id ?? "";
}

async function submitIgnoreRule() {
    if (!editingAgentId.value) {
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
            await system.agents.createIgnoreRule(editingAgentId.value, payload);
        } else {
            await system.agents.updateIgnoreRule(editingAgentId.value, ignoreRuleForm.id, payload);
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
    if (!editingAgentId.value) {
        return;
    }
    if (!window.confirm("确认删除这条 Ignore Rule 吗？")) {
        return;
    }
    ignoreRuleDeletingId.value = ruleId;
    ignoreRuleError.value = "";
    try {
        await system.agents.deleteIgnoreRule(editingAgentId.value, ruleId);
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
        const selected = llm.value.find((item) => item.config_id === form.image_understand_llm_ref_id);
        if (!selected || selected.model.type !== "chat_llm" || !selected.model.llm.supports_multimodal_input) {
            return "image_understand 需要选择一个支持多模态的模型";
        }
        return null;
    }
    if (!mainChatModelSupportsMultimodal.value) {
        return "image_understand 已启用时，主模型不支持多模态，请选择一个支持多模态的模型";
    }
    return null;
}

const RESERVED_TOOL_RUNTIME_INPUTS = new Set(["content", "message_event", "qq_ims_bot_adapter"]);

function isGeneratedToolId(value: string): boolean {
    return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value.trim());
}

const syncingToolIndex = ref<number | null>(null);

async function syncToolFromGraph(tool: AgentFormState["tools"][number], index: number) {
    syncingToolIndex.value = index;
    try {
        const result = await workflowApi.listDetailed();
        workflows.value = result.workflows;
        applyWorkflowSetMetadata(tool);
    } finally {
        syncingToolIndex.value = null;
    }
}

function handleToolTargetTypeChange(tool: AgentFormState["tools"][number]) {
    if (tool.targetType === "workflow_set" && tool.workflowName) {
        applyWorkflowSetMetadata(tool);
    }
}

function applyWorkflowSetMetadata(tool: AgentFormState["tools"][number]) {
    if (tool.targetType !== "workflow_set" || !tool.workflowName) {
        return;
    }
    const workflow = workflows.value.find((item) => item.name === tool.workflowName);
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
        const payload = buildAgentPayload(form);
        if (!payload.name) {
            alert("请填写 Agent 名称");
            return;
        }
        if (!form.llm_ref_id) {
            alert("请绑定一个模型配置");
            return;
        }
        if (form.type === "qq_chat" && !form.ims_bot_adapter_connection_id) {
            alert("QQ Chat Agent 需要绑定 Bot Adapter");
            return;
        }
        if (form.type === "qq_chat" && !form.web_search_engine_connection_id) {
            alert("QQ Chat Agent 需要绑定 Web Search Engine 连接");
            return;
        }
        const imageUnderstandError = validateImageUnderstandModelSelection();
        if (imageUnderstandError) {
            alert(imageUnderstandError);
            return;
        }
        if (form.type === "http_stream" && form.default_tools_enabled.web_search !== false && !form.http_web_search_engine_connection_id) {
            alert("启用 web_search 时，HTTP Stream Agent 需要绑定 Web Search Engine 连接");
            return;
        }
        if (form.type === "qq_chat" && form.weaviate_memory_connection_id && !form.embedding_model_ref_id) {
            alert("QQ Chat Agent 启用记忆库时需要绑定文本向量模型");
            return;
        }
        if (form.type === "http_stream" && form.http_weaviate_memory_connection_id && !form.http_embedding_model_ref_id) {
            alert("HTTP Stream Agent 启用记忆库时需要绑定文本向量模型");
            return;
        }
        if (form.id) {
            await system.agents.update(form.id, payload);
        } else {
            await system.agents.create(payload);
        }
        closeEditor();
        await load();
    } catch (error) {
        alert(`保存 Agent 失败: ${(error as Error).message}`);
    }
}

async function removeAgent(id: string) {
    if (!window.confirm("确认删除这个 Agent 吗？")) {
        return;
    }
    await system.agents.delete(id);
    if (form.id === id) {
        closeEditor();
    }
    await load();
}

async function startAgent(id: string) {
    try {
        console.log(`[Agent] 启动 Agent ${id}`);
        await system.agents.start(id);
        await load();
    } catch (error) {
        alert(`启动失败: ${(error as Error).message}`);
    }
}

async function stopAgent(id: string) {
    try {
        console.log(`[Agent] 停止 Agent ${id}`);
        await system.agents.stop(id);
        await load();
    } catch (error) {
        alert(`停止失败: ${(error as Error).message}`);
    }
}

async function toggleAgentRuntime(agent: AgentWithRuntime) {
    if (agent.runtime.status === "running") {
        await stopAgent(agent.config_id);
    } else {
        await startAgent(agent.config_id);
    }
}

function summarizeAgent(agent: AgentWithRuntime): Array<{ label: string; value: string; mono?: boolean }> {
    const items: Array<{ label: string; value: string; mono?: boolean }> = [
        { label: "Config ID", value: compactId(agent.config_id), mono: true },
        { label: "模型", value: llmName(agent), mono: false },
        { label: "自动启动", value: agent.auto_start ? "开启" : "关闭" },
    ];
    if (agent.runtime.instance_id) {
        items.push({ label: "Instance ID", value: compactId(agent.runtime.instance_id), mono: true });
    }
    const agentType = agent.agent_type as Record<string, unknown>;
    if (agent.agent_type.type === "qq_chat") {
        items.push(
            { label: "Bot Adapter", value: connectionName(String(agentType.ims_bot_adapter_connection_id ?? "")) || "未绑定" },
            { label: "Bot QQ", value: String(agent.qq_chat_profile?.bot_user_id ?? "") || "未知" },
            { label: "RustFS", value: connectionName(String(agentType.rustfs_connection_id ?? "")) || "未绑定" },
            { label: "Web Search", value: connectionName(String(agentType.web_search_engine_connection_id ?? "")) || "未绑定" },
            { label: "Bot Name", value: String(agentType.bot_name ?? "") || "未设置" },
            {
                label: "图片理解模型",
                value:
                    llmRefName(String(agentType.image_understand_llm_ref_id ?? "")) ||
                    llmRefName(String(agentType.llm_ref_id ?? "")) ||
                    "未绑定",
            },
            { label: "意图分类模型", value: llmRefName(String(agentType.intent_llm_ref_id ?? "")) || llmName(agent) },
            { label: "数学编程模型", value: llmRefName(String(agentType.math_programming_llm_ref_id ?? "")) || llmName(agent) },
            { label: "文本向量模型", value: llmRefName(String(agentType.embedding_model_ref_id ?? "")) || "未绑定" },
            { label: "记忆 Weaviate", value: connectionName(String(agentType.weaviate_memory_connection_id ?? "")) || "未绑定" },
            { label: "分词 Tokenizer", value: connectionName(String(agentType.tokenizer_connection_id ?? "")) || "未绑定" },
            { label: "System Prompt", value: String(agentType.system_prompt ?? "").trim() ? "已配置" : "未设置" },
            { label: "Max Message", value: String(agentType.max_message_length ?? 500) },
            { label: "Max Steer", value: String(agentType.max_steer_count ?? 4) },
        );
    } else {
        items.push(
            { label: "Bind", value: String(agentType.bind ?? "127.0.0.1:18080"), mono: true },
            { label: "API Key", value: String(agentType.api_key ?? "") ? "已配置" : "未设置" },
            { label: "Web Search", value: connectionName(String(agentType.web_search_engine_connection_id ?? "")) || "未绑定" },
            { label: "记忆向量模型", value: llmRefName(String(agentType.embedding_model_ref_id ?? "")) || "未绑定" },
            { label: "记忆 Weaviate", value: connectionName(String(agentType.weaviate_memory_connection_id ?? "")) || "未绑定" },
            { label: "web_search", value: (agentType.default_tools_enabled as Record<string, unknown> | undefined)?.web_search === false ? "关闭" : "开启" },
        );
    }
    if (agent.runtime.last_error) {
        items.push({ label: "最近错误", value: agent.runtime.last_error });
    }
    return items;
}

function connectionName(id: string): string {
    return connections.value.find((item) => item.config_id === id)?.name ?? "";
}

function llmName(agent: AgentWithRuntime): string {
    const agentType = agent.agent_type as Record<string, unknown>;
    const llmId = String(agentType.llm_ref_id ?? "");
    return llmRefName(llmId) || "未绑定";
}

function llmRefName(id: string): string {
    return llm.value.find((item) => item.config_id === id)?.name ?? "";
}

function botAvatarUrl(agent: AgentWithRuntime): string {
    if (agent.agent_type.type !== "qq_chat") {
        return "";
    }
    return String(agent.qq_chat_profile?.bot_avatar_url ?? "");
}

function runtimeBadgeText(agent: AgentWithRuntime): string {
    switch (agent.runtime.status) {
        case "running":
            return agent.runtime.instance_id
                ? `已启动 (${summarizeIds([agent.runtime.instance_id])})`
                : "已启动";
        case "stopped":
            return "已停止";
        case "starting":
            return "启动中";
        case "error":
            return "启动失败";
        default:
            return agent.runtime.status;
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
.agent-loading-state {
    min-height: 180px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--admin-subtle);
}

.agent-loading-spinner {
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
</style>
