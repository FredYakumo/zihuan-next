<template>
    <t-form class="agent-service-form" label-align="top">
        <t-card class="agent-service-form-section" :bordered="false">
            <template #title>{{ form.type === 'qq_chat' ? 'RoleService 配置' : '基本信息' }}</template>
            <t-form-item v-if="form.type === 'workspace'" label="头像" class="agent-service-form-item-full">
                <div class="agent-service-avatar-row">
                    <input type="file" accept="image/*" style="display: none" @change="handleAvatarFileSelect" />
                    <button type="button" class="agent-service-avatar-trigger"
                        :title="form.avatar_url ? '更换头像' : '上传头像'" @click="openAvatarEditor">
                        <img v-if="form.avatar_url" :src="getAvatarDisplayUrl(form.avatar_url)"
                            alt="Avatar preview" class="agent-service-avatar-preview" />
                        <div v-else class="agent-service-avatar-placeholder">{{ form.name ?
                            form.name.slice(0, 1).toUpperCase() : 'A' }}</div>
                        <div class="agent-service-avatar-overlay">
                            <EditIcon />
                        </div>
                    </button>
                </div>
            </t-form-item>
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
                        <t-option v-for="item in botConnections" :key="item.config_id" :value="item.config_id"
                            :label="item.name" />
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
        </t-card>

        <ServiceModelConfig :form="form" :chat-models="chatModels"
            :multimodal-chat-models="multimodalChatModels" :tokenizer-connections="tokenizerConnections"
            @primary-model-change="handlePrimaryModelChange"
            @image-understand-model-change="handleImageUnderstandModelChange"
            @memory-backend-change="handleMemoryBackendChange" />

        <t-card v-if="form.type === 'workspace' && (form.workspace_memory_enabled || form.default_tools_enabled.web_search)"
            class="agent-service-form-section" :bordered="false">
            <template #title>检索增强生成</template>
            <div class="agent-service-form-grid">
                <t-form-item v-if="form.workspace_memory_backend === 'retrieval_store'" label="文本向量模型" required
                    :status="!form.workspace_embedding_model_ref_id ? 'error' : undefined"
                    :help="!form.workspace_embedding_model_ref_id ? '必须选择文本向量模型。' : undefined">
                    <t-select v-model="form.workspace_embedding_model_ref_id" placeholder="请选择文本向量模型">
                        <t-option v-for="item in embeddingModels" :key="item.config_id" :value="item.config_id"
                            :label="item.name" />
                    </t-select>
                </t-form-item>
                <t-form-item v-if="form.workspace_memory_backend === 'retrieval_store'" label="检索数据库" required
                    :status="!form.workspace_retrieval_store_id ? 'error' : undefined"
                    :help="!form.workspace_retrieval_store_id ? '必须选择记忆库连接。' : undefined">
                    <t-select v-model="form.workspace_retrieval_store_id" placeholder="请选择记忆库连接"
                        @change="handleWorkspaceRetrievalStoreChange">
                        <t-option class="agent-service-add-retrieval-option" value="__add_retrieval_database__"
                            label="新增检索数据库">
                            <span class="agent-service-add-model-option-content">
                                <AddIcon />新增检索数据库
                            </span>
                        </t-option>
                        <t-option v-for="item in retrievalConnections" :key="item.config_id" :value="item.config_id"
                            :label="item.name" />
                    </t-select>
                </t-form-item>
                <t-form-item v-if="form.default_tools_enabled.web_search" label="Web Search Engine" required
                    :status="!form.web_search_engine_connection_id ? 'error' : undefined"
                    :help="!form.web_search_engine_connection_id ? '启用联网搜索后必须选择连接。' : undefined">
                    <t-select v-model="form.web_search_engine_connection_id" placeholder="请选择"
                        @change="handleWebSearchChange">
                        <t-option class="agent-service-add-web-search-option" value="__add_web_search__"
                            label="新增 Web Search">
                            <span class="agent-service-add-model-option-content">
                                <AddIcon />新增 Web Search
                            </span>
                        </t-option>
                        <t-option value="" label="请选择" />
                        <t-option v-for="item in webSearchEngineConnections" :key="item.config_id"
                            :value="item.config_id" :label="item.name" />
                    </t-select>
                </t-form-item>
            </div>
        </t-card>

        <t-card v-if="form.type === 'workspace'" class="agent-service-form-section" :bordered="false">
            <template #title>计划任务</template>
            <div class="agent-service-form-grid">
                <t-form-item label="关系型数据库">
                    <RelationalDatabaseSelect v-model="form.rdb_id" placeholder="不使用" clearable />
                </t-form-item>
            </div>
            <ScheduledJobsEditor :jobs="form.scheduled_jobs" :available-jobs="schedulerJobs"
                :rdb-configured="Boolean(form.rdb_id)" @add="addScheduledJob" @remove="removeScheduledJob" />
        </t-card>

        <template v-if="form.type === 'qq_chat'">
            <t-card class="agent-service-form-section" :bordered="false">
                <template #title>RAG 配置</template>
                <div class="agent-service-form-grid">
                    <t-form-item label="关系型数据库">
                        <RelationalDatabaseSelect v-model="form.rdb_id" placeholder="不使用" clearable />
                    </t-form-item>
                    <t-form-item label="RustFS">
                        <t-select v-model="form.rustfs_connection_id" placeholder="不使用" clearable>
                            <t-option value="" label="不使用" />
                            <t-option v-for="item in rustfsConnections" :key="item.config_id" :value="item.config_id"
                                :label="item.name" />
                        </t-select>
                    </t-form-item>
                    <t-form-item label="Web Search Engine" required>
                        <t-select v-model="form.web_search_engine_connection_id" placeholder="请选择"
                            @change="handleWebSearchChange">
                            <t-option class="agent-service-add-web-search-option" value="__add_web_search__"
                                label="新增 Web Search">
                                <span class="agent-service-add-model-option-content">
                                    <AddIcon />新增 Web Search
                                </span>
                            </t-option>
                            <t-option value="" label="请选择" />
                            <t-option v-for="item in webSearchEngineConnections" :key="item.config_id"
                                :value="item.config_id" :label="item.name" />
                        </t-select>
                    </t-form-item>
                    <t-form-item label="检索数据库">
                        <t-select v-model="form.retrieval_store_id" placeholder="不使用" clearable
                            @change="handleRetrievalStoreChange">
                            <t-option class="agent-service-add-retrieval-option" value="__add_retrieval_database__"
                                label="新增检索数据库">
                                <span class="agent-service-add-model-option-content">
                                    <AddIcon />新增检索数据库
                                </span>
                            </t-option>
                            <t-option value="" label="不使用" />
                            <t-option value="__local_markdown__" label="本地 Markdown" />
                            <t-option v-for="item in retrievalConnections" :key="item.config_id"
                                :value="item.config_id" :label="item.name" />
                        </t-select>
                    </t-form-item>
                    <t-form-item label="Embedding 来源">
                        <t-select v-model="form.embedding_mode" placeholder="不使用">
                            <t-option value="none" label="不使用" />
                            <t-option value="local" label="本地向量模型" />
                            <t-option value="http" label="HTTP 服务" />
                        </t-select>
                    </t-form-item>
                    <t-form-item v-if="form.embedding_mode === 'local'" label="本地 Embedding 模型">
                        <t-select v-model="form.embedding_model_ref_id" placeholder="请选择">
                            <t-option v-for="item in embeddingModels" :key="item.config_id" :value="item.config_id"
                                :label="item.name" />
                        </t-select>
                    </t-form-item>
                    <t-form-item v-if="form.embedding_mode === 'http'" label="Embedding API 地址"
                        class="agent-service-form-item-full">
                        <t-input v-model="form.embedding_http_endpoint"
                            placeholder="https://api.example.com/v1/embeddings" />
                        <div class="agent-service-form-hint">兼容 OpenAI /v1/embeddings 接口</div>
                    </t-form-item>
                    <t-form-item v-if="form.embedding_mode === 'http'" label="Embedding 模型名">
                        <t-input v-model="form.embedding_http_model" placeholder="例如 qwen3-embedding:0.6b" />
                    </t-form-item>
                    <t-form-item v-if="form.embedding_mode === 'http'" label="API Key">
                        <ConnectionCredentialInput v-model="form.embedding_http_api_key" placeholder="可选" />
                    </t-form-item>
                    <t-form-item v-if="form.embedding_mode === 'http'" label="超时（秒）">
                        <t-input-number v-model="form.embedding_http_timeout_secs" :min="1" />
                    </t-form-item>
                    <t-form-item v-if="form.embedding_mode === 'http'" label="重试次数">
                        <t-input-number v-model="form.embedding_http_retry_count" :min="0" />
                    </t-form-item>
                </div>
            </t-card>

            <t-card class="agent-service-form-section" :bordered="false">
                <template #title>Prompt engineering</template>
                <div class="agent-service-form-grid">
                    <t-form-item label="System Prompt" class="agent-service-form-item-full">
                        <t-textarea v-model="form.system_prompt"
                            placeholder="可选。会追加在 QQ Chat RoleService 的通用系统规则后面。" />
                    </t-form-item>
                    <t-form-item label="自然语言回复 Prompt" class="agent-service-form-item-full">
                        <t-textarea v-model="form.natural_language_reply_system_prompt"
                            placeholder="可选。专门给自然语言回复模型使用的系统提示词。" />
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
                        <div class="agent-service-form-hint">默认4次</div>
                    </t-form-item>
                    <t-form-item label="群聊消息窗口数量">
                        <t-input-number v-model="form.group_message_window_count" :min="0" />
                    </t-form-item>
                    <t-form-item label="计划任务" class="agent-service-form-item-full">
                        <ScheduledJobsEditor :jobs="form.scheduled_jobs" :available-jobs="schedulerJobs"
                            :rdb-configured="Boolean(form.rdb_id)" @add="addScheduledJob"
                            @remove="removeScheduledJob" />
                    </t-form-item>
                </div>
                <div class="agent-service-form-grid">
                    <t-form-item label="情绪维度" class="agent-service-form-item-full">
                        <div class="agent-service-form-hint">Service 的情绪可以由一个或者多个维度组成，这些维度共同构成 Agent
                            的决策、行为和输出语言风格。</div>
                        <div class="agent-service-form-hint" style="margin-top: 6px">当前已配置 {{
                            form.emotion_dimensions.length
                            }} 个维度。</div>
                        <t-button variant="text" style="margin-top: 6px; padding-left: 0"
                            @click="openEmotionDimensionsModal">配置情绪维度</t-button>
                    </t-form-item>
                    <t-form-item label="Rate Limit" class="agent-service-form-item-full">
                        <div class="agent-service-form-hint">按 N 天/小时/分钟限制调用次数，计数跟随用户（跨群与私聊共享），优先级：用户 &gt;
                            群组 &gt; 默认。</div>
                        <t-button variant="text" style="margin-top: 6px; padding-left: 0"
                            @click="openRateLimitModal">编辑
                            Rate Limit</t-button>
                    </t-form-item>
                    <t-form-item label="Ignore Rules" class="agent-service-form-item-full">
                        <div class="agent-service-form-hint">命中后仅做消息存储，不回复、不进入推理流程。</div>
                        <t-button variant="text" style="margin-top: 6px; padding-left: 0"
                            :disabled="Boolean(ignoreRulesDisabledReason)"
                            @click="openIgnoreRulesModal()">管理 Ignore
                            Rules</t-button>
                        <div v-if="ignoreRulesDisabledReason" class="agent-service-form-hint"
                            style="margin-top: 4px">
                            <InfoCircleIcon /> {{ ignoreRulesDisabledReason }}
                        </div>
                    </t-form-item>
                </div>
            </t-card>
        </template>

        <t-card class="agent-service-form-section" :bordered="false">
            <template #title>
                <div class="agent-service-section-title-row">
                    <span>工具和能力</span>
                    <div class="agent-service-tools-toolbar">
                        <t-input v-model="toolSearchQuery" placeholder="搜索工具" clearable
                            class="agent-service-tools-search-input" />
                        <t-button variant="text" @click="openNewTool">增加工具</t-button>
                    </div>
                </div>
            </template>
            <div v-if="filteredToolRows.length === 0" class="agent-service-empty-state">没有匹配的工具。</div>
            <t-table v-else :data="filteredToolRows" :columns="toolColumns" :hover="true" :pagination="false"
                row-key="key" table-layout="fixed" :row-class-name="toolRowClassName">
                <template #enabled="{ row }">
                    <t-checkbox v-if="row.kind === 'builtin'" v-model="form.default_tools_enabled[row.id]" />
                    <t-checkbox v-else-if="row.tool" v-model="row.tool.enabled" />
                </template>
                <template #actions="{ row }">
                    <t-button variant="text" size="small"
                        @click="row.kind === 'builtin' ? openDefaultToolEditModal(row.id) : openToolEdit(row.toolIndex ?? -1)">编辑</t-button>
                    <t-button v-if="row.kind === 'custom'" variant="text" theme="danger" size="small"
                        @click="removeTool(row.toolIndex ?? -1)">移除</t-button>
                </template>
            </t-table>
        </t-card>
        <t-card v-if="form.type === 'qq_chat'" class="agent-service-form-section" :bordered="false">
            <template #title>工具调用上限回文</template>
            <t-form-item label="达到调用上限回文（可选）" class="agent-service-form-item-full">
                <t-textarea v-model="form.tool_session_limit_message"
                    placeholder="当前工具已经达到 [{limit_scope}]的调用次数限制，但是你不要把这个直接当成自然语言输出给用户，仅当用户问及详细原因时，再告知" />
            </t-form-item>
            <div class="agent-service-form-hint">留空则使用默认提示。可用 {limit_scope} 占位符表示限制范围（会替换为"单次会话"或"用户"）。</div>
        </t-card>
    </t-form>
</template>

<script lang="ts">
import { defineComponent, type PropType } from "vue";
import { AddIcon, EditIcon, InfoCircleIcon } from "tdesign-icons-vue-next";
import ConnectionCredentialInput from "./ConnectionCredentialInput.vue";
import RelationalDatabaseSelect from "./RelationalDatabaseSelect.vue";
import ScheduledJobsEditor from "./ScheduledJobsEditor.vue";
import ServiceModelConfig from "./ServiceModelConfig.vue";
import type { UseRoleServicePageReturn } from "../agents/roleServicePage";

/**
 * Shared form body of the RoleService create/edit drawers.
 *
 * The two drawers used to inline their own copies of this form and drifted apart; both now
 * render this component with the page view-model built by `useRoleServicePage` — the same
 * `form` state, connection option lists, and open/change handlers the drawers used to bind.
 */
export default defineComponent({
    components: {
        AddIcon,
        ConnectionCredentialInput,
        EditIcon,
        InfoCircleIcon,
        RelationalDatabaseSelect,
        ScheduledJobsEditor,
        ServiceModelConfig,
    },
    props: {
        page: { type: Object as PropType<UseRoleServicePageReturn>, required: true },
    },
    setup(props) {
        // Spread the view-model into the setup return so the template keeps the exact
        // bindings the inlined copies had; refs stay the shared store instances.
        return { ...props.page };
    },
});
</script>

<style scoped lang="scss">
@use "../agents/role-service" as *;
</style>
