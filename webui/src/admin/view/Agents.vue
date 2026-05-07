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
              <label class="field-check"><input v-model="form.is_default" type="checkbox" />默认 Agent</label>
            </div>

            <div class="field">
              <label>模型配置</label>
              <select v-model="form.llm_ref_id">
                <option value="">请选择</option>
                <option v-for="item in llm" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
              </select>
            </div>

            <template v-if="form.type === 'qq_chat'">
              <div class="field">
                <label>意图分类模型</label>
                <select v-model="form.intent_llm_ref_id">
                  <option value="">回退主模型</option>
                  <option v-for="item in llm" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                </select>
              </div>
              <div class="field">
                <label>数学编程模型</label>
                <select v-model="form.math_programming_llm_ref_id">
                  <option value="">回退主模型</option>
                  <option v-for="item in llm" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                </select>
              </div>
              <div class="field">
                <label>Bot Adapter</label>
                <select v-model="form.ims_bot_adapter_connection_id">
                  <option value="">请选择</option>
                  <option v-for="item in botConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
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
                  <option v-for="item in rustfsConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                </select>
              </div>
              <div class="field">
                <label>Tavily Connection</label>
                <select v-model="form.tavily_connection_id">
                  <option value="">请选择</option>
                  <option v-for="item in tavilyConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                </select>
              </div>
              <div class="field">
                <label>MySQL Connection</label>
                <select v-model="form.mysql_connection_id">
                  <option value="">不使用</option>
                  <option v-for="item in mysqlConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                </select>
              </div>
              <div class="field">
                <label>Weaviate Connection</label>
                <select v-model="form.weaviate_connection_id">
                  <option value="">不使用</option>
                  <option v-for="item in weaviateConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                </select>
              </div>
              <div class="field">
                <label>Weaviate Image Connection</label>
                <select v-model="form.weaviate_image_connection_id">
                  <option value="">不使用</option>
                  <option v-for="item in weaviateConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                </select>
              </div>
              <div class="field"><label>Max Message Length</label><input v-model.number="form.max_message_length" type="number" min="1" /></div>
              <div class="field"><label>Compact Context Length</label><input v-model.number="form.compact_context_length" type="number" min="0" /></div>
            </template>

            <template v-else>
              <div class="field"><label>Bind</label><input v-model="form.http_bind" placeholder="127.0.0.1:18080" /></div>
              <div class="field"><label>API Key</label><input v-model="form.http_api_key" /></div>
            </template>
          </div>

          <div v-if="form.type === 'qq_chat'" class="editor-card" style="margin-top: 12px;">
            <div class="split-header">
              <div>
                <h3>默认工具</h3>
              </div>
            </div>
            <div class="list" style="margin-top: 12px;">
              <label
                v-for="tool in qqChatDefaultTools"
                :key="tool.id"
                class="field-check"
                style="display: flex; align-items: flex-start; gap: 8px; margin-bottom: 8px;"
              >
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
                    <label>目标类型</label>
                    <select v-model="tool.targetType">
                      <option value="workflow_set">workflow_set</option>
                      <option value="file_path">file_path</option>
                      <option value="inline_graph">inline_graph</option>
                    </select>
                  </div>
                  <div class="field field-check"><input v-model="tool.enabled" type="checkbox" />启用该工具</div>
                  <div v-if="tool.targetType === 'workflow_set'" class="field-full">
                    <label>Workflow Set 名称</label>
                    <select v-model="tool.workflowName">
                      <option value="">请选择</option>
                      <option v-for="workflow in workflows" :key="workflow.name" :value="workflow.name">
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
                    <label>Parameters JSON</label>
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
          <button
            v-for="type in agentTypes"
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

    <section v-if="agents.length > 0" class="panel">
      <div class="connection-grid connection-grid--agents" style="margin-top: 0;">
        <article
          v-for="agent in agents"
          :key="agent.config_id"
          :class="['connection-card', { 'connection-card--editing': isEditingAgent(agent.config_id) }]"
        >
          <template v-if="isEditingAgent(agent.config_id)">
            <div class="connection-card-header connection-card-header--stacked">
              <div class="connection-card-header-top">
                <div class="connection-card-badges">
                  <span class="badge">{{ form.type }}</span>
                  <span class="badge" :class="form.enabled ? 'success' : ''">{{ form.enabled ? "已启用" : "已停用" }}</span>
                  <span v-if="form.is_default" class="badge">default</span>
                </div>
                <div class="inline-actions connection-card-edit-actions">
                  <button class="btn primary connection-card-compact-btn" @click="submitForm">保存</button>
                  <button class="btn ghost connection-card-compact-btn" @click="closeEditor">取消</button>
                </div>
              </div>
              <div class="connection-card-title-edit">
                <input v-model="form.name" class="connection-card-inline-input connection-card-inline-input--title" />
              </div>
            </div>

            <div class="connection-card-body">
              <div class="key-value connection-card-edit-row">
                <strong>类型</strong>
                <select v-model="form.type" class="connection-card-inline-input">
                  <option value="qq_chat">QQ Chat Agent</option>
                  <option value="http_stream">HTTP Stream Agent</option>
                </select>
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>启用</strong>
                <label class="connection-card-inline-check">
                  <input :id="`agent-enabled-${agent.config_id}`" v-model="form.enabled" type="checkbox" />
                  <span>{{ form.enabled ? "已启用" : "已停用" }}</span>
                </label>
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>自启</strong>
                <label class="connection-card-inline-check">
                  <input :id="`agent-auto-start-${agent.config_id}`" v-model="form.auto_start" type="checkbox" />
                  <span>{{ form.auto_start ? "开启" : "关闭" }}</span>
                </label>
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>默认</strong>
                <label class="connection-card-inline-check">
                  <input :id="`agent-default-${agent.config_id}`" v-model="form.is_default" type="checkbox" />
                  <span>{{ form.is_default ? "是" : "否" }}</span>
                </label>
              </div>
              <div class="key-value connection-card-edit-row">
                <strong>模型</strong>
                <select v-model="form.llm_ref_id" class="connection-card-inline-input">
                  <option value="">请选择</option>
                  <option v-for="item in llm" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                </select>
              </div>

              <template v-if="form.type === 'qq_chat'">
                <div class="key-value connection-card-edit-row">
                  <strong>意图分类模型</strong>
                  <select v-model="form.intent_llm_ref_id" class="connection-card-inline-input">
                    <option value="">回退主模型</option>
                    <option v-for="item in llm" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                  </select>
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>数学编程模型</strong>
                  <select v-model="form.math_programming_llm_ref_id" class="connection-card-inline-input">
                    <option value="">回退主模型</option>
                    <option v-for="item in llm" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                  </select>
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Bot Adapter</strong>
                  <select v-model="form.ims_bot_adapter_connection_id" class="connection-card-inline-input">
                    <option value="">请选择</option>
                    <option v-for="item in botConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                  </select>
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Bot Name</strong>
                  <input v-model="form.bot_name" class="connection-card-inline-input" />
                </div>
                <div class="key-value connection-card-edit-row" style="align-items: flex-start;">
                  <strong>System Prompt</strong>
                  <textarea
                    v-model="form.system_prompt"
                    class="connection-card-inline-input"
                    placeholder="可选。会追加在 QQ Chat Agent 的通用系统规则后面。"
                    style="min-height: 110px;"
                  />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>RustFS</strong>
                  <select v-model="form.rustfs_connection_id" class="connection-card-inline-input">
                    <option value="">不使用</option>
                    <option v-for="item in rustfsConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                  </select>
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Tavily</strong>
                  <select v-model="form.tavily_connection_id" class="connection-card-inline-input">
                    <option value="">请选择</option>
                    <option v-for="item in tavilyConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                  </select>
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>MySQL</strong>
                  <select v-model="form.mysql_connection_id" class="connection-card-inline-input">
                    <option value="">不使用</option>
                    <option v-for="item in mysqlConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                  </select>
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Weaviate</strong>
                  <select v-model="form.weaviate_connection_id" class="connection-card-inline-input">
                    <option value="">不使用</option>
                    <option v-for="item in weaviateConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                  </select>
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Image DB</strong>
                  <select v-model="form.weaviate_image_connection_id" class="connection-card-inline-input">
                    <option value="">不使用</option>
                    <option v-for="item in weaviateConnections" :key="item.config_id" :value="item.config_id">{{ item.name }}</option>
                  </select>
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Max Msg</strong>
                  <input v-model.number="form.max_message_length" class="connection-card-inline-input" type="number" min="1" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>Compact</strong>
                  <input v-model.number="form.compact_context_length" class="connection-card-inline-input" type="number" min="0" />
                </div>

                <div class="editor-card" style="margin-top: 8px;">
                  <div class="split-header">
                    <div>
                      <h3>默认工具</h3>
                    </div>
                  </div>
                  <div class="list" style="margin-top: 14px;">
                    <label
                      v-for="tool in qqChatDefaultTools"
                      :key="tool.id"
                      class="field-check"
                      style="display: flex; align-items: flex-start; gap: 8px; margin-bottom: 8px;"
                    >
                      <input v-model="form.default_tools_enabled[tool.id]" type="checkbox" />
                      <span>
                        <strong>{{ tool.label }}</strong>
                        <span class="muted" style="display: block;">{{ tool.description }}</span>
                      </span>
                    </label>
                  </div>
                </div>
              </template>

              <template v-else>
                <div class="key-value connection-card-edit-row">
                  <strong>Bind</strong>
                  <input v-model="form.http_bind" class="connection-card-inline-input" placeholder="127.0.0.1:18080" />
                </div>
                <div class="key-value connection-card-edit-row">
                  <strong>API Key</strong>
                  <input v-model="form.http_api_key" class="connection-card-inline-input" />
                </div>
              </template>

              <div class="editor-card">
                <div class="split-header">
                  <div>
                    <h3>工具配置</h3>
                  </div>
                  <button class="btn ghost" @click="addTool">新增工具</button>
                </div>
                <div class="list" style="margin-top: 12px;">
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
                        <label>目标类型</label>
                        <select v-model="tool.targetType">
                          <option value="workflow_set">workflow_set</option>
                          <option value="file_path">file_path</option>
                          <option value="inline_graph">inline_graph</option>
                        </select>
                      </div>
                      <div class="field field-check"><input v-model="tool.enabled" type="checkbox" />启用该工具</div>
                      <div v-if="tool.targetType === 'workflow_set'" class="field-full">
                        <label>Workflow Set 名称</label>
                        <select v-model="tool.workflowName">
                          <option value="">请选择</option>
                          <option v-for="workflow in workflows" :key="workflow.name" :value="workflow.name">
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
                        <label>Parameters JSON</label>
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
          </template>

          <template v-else>
            <div class="connection-card-header connection-card-header--stacked">
              <div class="connection-card-header-top">
                <div class="connection-card-badges">
                  <span class="badge">{{ agent.agent_type.type }}</span>
                  <span class="badge" :class="agent.enabled ? 'success' : ''">{{ agent.enabled ? "已启用" : "已停用" }}</span>
                  <span class="badge" :class="statusTone(agent.runtime.status)">{{ runtimeBadgeText(agent) }}</span>
                  <span v-if="agent.is_default" class="badge">default</span>
                </div>
                <div class="inline-actions connection-card-display-actions">
                  <button class="btn ghost connection-card-compact-btn" @click="editAgent(agent)">编辑</button>
                  <button
                    class="btn connection-card-compact-btn"
                    @click="toggleAgentRuntime(agent)"
                  >
                    {{ agent.runtime.status === "running" ? "停止" : "启动" }}
                  </button>
                  <button class="btn warn connection-card-compact-btn" @click="removeAgent(agent.config_id)">删除</button>
                </div>
              </div>
              <div style="display: flex; align-items: center; gap: 10px;">
                <img
                  v-if="botAvatarUrl(agent)"
                  :src="botAvatarUrl(agent)"
                  alt="bot avatar"
                  style="width: 36px; height: 36px; border-radius: 999px; border: 1px solid var(--line); object-fit: cover; background: var(--surface-soft);"
                />
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
          </template>
        </article>
      </div>
    </section>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";

import { system, workflows as workflowApi, type AgentWithRuntime, type ConnectionConfig, type LlmConfig } from "../../api/client";
import {
  agentFormFromConfig,
  buildAgentPayload,
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
const connections = ref<ConnectionConfig[]>([]);
const llm = ref<LlmConfig[]>([]);
const workflows = ref<Array<{ name: string; file: string; cover_url: string | null; display_name: string | null; description: string | null; version: string | null }>>([]);
const form = reactive<AgentFormState>(defaultAgentForm());
const editingAgentId = ref("");
const showCreatePicker = ref(false);
const showCreateForm = ref(false);
const qqChatDefaultTools = QQ_CHAT_DEFAULT_TOOLS;

const botConnections = computed(() => connections.value.filter((item) => isBotAdapterConnectionType(String(item.kind.type ?? ""))));
const rustfsConnections = computed(() => connections.value.filter((item) => item.kind.type === "rustfs"));
const tavilyConnections = computed(() => connections.value.filter((item) => item.kind.type === "tavily"));
const mysqlConnections = computed(() => connections.value.filter((item) => item.kind.type === "mysql"));
const weaviateConnections = computed(() => connections.value.filter((item) => item.kind.type === "weaviate"));

function resetForm() {
  Object.assign(form, defaultAgentForm());
}

function clearEditingAgent() {
  editingAgentId.value = "";
}

function isEditingAgent(agentId: string) {
  return editingAgentId.value === agentId;
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
  resetForm();
  clearEditingAgent();
  showCreatePicker.value = false;
  showCreateForm.value = false;
}

async function load() {
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
}

function editAgent(agent: AgentWithRuntime) {
  Object.assign(form, agentFormFromConfig(agent));
  editingAgentId.value = agent.config_id;
  showCreatePicker.value = false;
  showCreateForm.value = false;
  window.scrollTo({ top: 0, behavior: "smooth" });
}

function addTool() {
  form.tools.push(defaultToolForm());
}

function removeTool(index: number) {
  form.tools.splice(index, 1);
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
    if (form.type === "qq_chat" && !form.tavily_connection_id) {
      alert("QQ Chat Agent 需要绑定 Tavily 连接");
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
      { label: "Tavily", value: connectionName(String(agentType.tavily_connection_id ?? "")) || "未绑定" },
      { label: "Bot Name", value: String(agentType.bot_name ?? "") || "未设置" },
      { label: "意图分类模型", value: llmRefName(String(agentType.intent_llm_ref_id ?? "")) || llmName(agent) },
      { label: "数学编程模型", value: llmRefName(String(agentType.math_programming_llm_ref_id ?? "")) || llmName(agent) },
      { label: "System Prompt", value: String(agentType.system_prompt ?? "").trim() ? "已配置" : "未设置" },
      { label: "Max Message", value: String(agentType.max_message_length ?? 500) },
    );
  } else {
    items.push(
      { label: "Bind", value: String(agentType.bind ?? "127.0.0.1:18080"), mono: true },
      { label: "API Key", value: String(agentType.api_key ?? "") ? "已配置" : "未设置" },
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
      return "错误";
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
