<template>
  <section class="page agent-config-page">
    <AdminPageHeader title="Agent 配置">
      <t-button theme="primary" @click="showTypePicker = true">新增 Agent</t-button>
    </AdminPageHeader>
    <t-card bordered>
      <t-table :data="subagents" :columns="columns" row-key="id" :loading="loading">
        <template #name="{ row }"><div class="agent-name-cell"><span>{{ row.name }}</span><t-tag size="small" variant="light" theme="primary">SubAgent</t-tag><t-tag v-if="row.builtin" size="small" variant="light" theme="success">Built-in</t-tag></div></template>
        <template #tools="{ row }">{{ row.tool_ids.length }} 个工具</template>
        <template #actions="{ row }"><t-button variant="text" size="small" @click="openEditor(row.id)">编辑</t-button><t-popconfirm content="确认删除这个 SubAgent 吗？默认 Agent 将在下次启动时自动恢复。" @confirm="remove(row.id)"><t-button variant="text" theme="danger" size="small">删除</t-button></t-popconfirm></template>
        <template #empty><div class="agent-config-empty">{{ listError ? "SubAgent 列表加载失败。" : "暂无 SubAgent。" }}</div></template>
      </t-table>
    </t-card>
    <t-alert v-if="listError" theme="error" :message="listError" class="agent-config-alert" />
    <t-dialog v-model:visible="showTypePicker" header="选择 Agent 类型" :confirm-btn="null" cancel-btn="取消" width="440px"><t-button block variant="outline" class="agent-type-card" @click="startCreate"><strong>SubAgent</strong><span>通过提示词、输入输出和工具列表定义全局子 Agent。</span></t-button></t-dialog>
    <t-dialog v-model:visible="editorVisible" :header="isCreating ? '新增 SubAgent' : `${form.id} 配置`" :confirm-btn="{ content: '保存', loading: saving }" cancel-btn="取消" width="760px" @confirm="save">
      <t-form label-align="top"><div class="form-grid"><t-form-item label="ID" required><t-input v-model="form.id" :disabled="!isCreating" placeholder="例如 research_agent" /></t-form-item><t-form-item label="名称" required><t-input v-model="form.name" /></t-form-item></div>
        <t-form-item label="描述"><t-input v-model="form.description" placeholder="提供给 LLM 的工具描述，留空则使用名称" /></t-form-item>
        <div class="form-grid"><t-form-item label="模型用途"><t-input v-model="form.llm_kind" placeholder="main / math_programming" /></t-form-item><t-form-item label="运行时长"><t-select v-model="form.run_duration"><t-option value="Short" label="Short" /><t-option value="Long" label="Long" /></t-select></t-form-item></div>
        <div class="form-grid"><t-form-item label="输出模式"><t-select v-model="form.output_mode"><t-option value="json_ports" label="JSON 输出端口" /><t-option value="text" label="纯文本" /></t-select></t-form-item><t-form-item label="工具进度提示"><t-input v-model="form.progress_message" placeholder="运行前写入 dashboard 的进度文本" /></t-form-item></div>
        <t-form-item label="输入"><div class="port-list"><div v-for="(port, index) in form.inputs" :key="`input-${index}`" class="port-row"><t-input v-model="port.name" placeholder="字段名" /><t-select v-model="port.data_type"><t-option v-for="type in portTypes" :key="type" :value="type" :label="type" /></t-select><t-input v-model="port.description" placeholder="描述" /><t-checkbox v-model="port.required">必填</t-checkbox><t-button variant="text" theme="danger" @click="removePort('inputs', index)">删除</t-button></div><t-button variant="dashed" size="small" @click="addPort('inputs')">新增输入</t-button></div></t-form-item>
        <t-form-item label="系统提示词"><t-textarea v-model="form.system_prompt" :autosize="{ minRows: 5, maxRows: 12 }" /></t-form-item>
        <t-form-item label="用户提示词模板"><t-textarea v-model="form.user_prompt" :autosize="{ minRows: 3, maxRows: 10 }" placeholder="使用 {输入名} 占位；留空则使用默认 JSON 封装" /></t-form-item>
        <t-form-item label="输出"><div class="port-list"><div v-for="(port, index) in form.outputs" :key="`output-${index}`" class="port-row"><t-input v-model="port.name" placeholder="字段名" /><t-select v-model="port.data_type"><t-option v-for="type in portTypes" :key="type" :value="type" :label="type" /></t-select><t-input v-model="port.description" placeholder="描述" /><t-checkbox v-model="port.required">必填</t-checkbox><t-button variant="text" theme="danger" @click="removePort('outputs', index)">删除</t-button></div><t-button variant="dashed" size="small" @click="addPort('outputs')">新增输出</t-button></div></t-form-item>
        <t-form-item label="工具列表"><t-checkbox-group v-model="form.tool_ids"><t-checkbox v-for="tool in availableTools" :key="tool.id" :value="tool.id">{{ tool.name }}</t-checkbox></t-checkbox-group></t-form-item><t-form-item label="节点图工具"><t-checkbox v-model="form.include_graph_tools">合并角色定义的节点图工具</t-checkbox></t-form-item><div v-if="error" class="agent-error">{{ error }}</div></t-form>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { system, type SubAgentDefinition } from "../../api/client";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { QQ_CHAT_DEFAULT_TOOLS, WORKSPACE_DEFAULT_TOOLS } from "../model";

const portTypes = ["String", "Integer", "Float", "Boolean", "Json"];
// Callable tools that the role-service default tool lists do not cover because they are configured
// through their own switches; a definition may still reference them.
const SWITCH_CONFIGURED_TOOLS = [{ id: "image_understand", name: "image_understand" }];
const availableTools = computed(() => {
  const defaults = [...QQ_CHAT_DEFAULT_TOOLS.map((tool) => ({ id: tool.id, name: tool.label })), ...WORKSPACE_DEFAULT_TOOLS.filter((tool) => !QQ_CHAT_DEFAULT_TOOLS.some((item) => item.id === tool.id)).map((tool) => ({ id: tool.id, name: tool.label })), ...SWITCH_CONFIGURED_TOOLS.filter((tool) => !QQ_CHAT_DEFAULT_TOOLS.some((item) => item.id === tool.id) && !WORKSPACE_DEFAULT_TOOLS.some((item) => item.id === tool.id))];
  // Existing sub-agents are callable tools too, so a definition can reference another.
  const known = new Set(defaults.map((tool) => tool.id));
  const nested = subagents.value.filter((agent) => !known.has(agent.id)).map((agent) => ({ id: agent.id, name: agent.name }));
  return [...defaults, ...nested];
});
const subagents = ref<SubAgentDefinition[]>([]); const loading = ref(false); const showTypePicker = ref(false); const editorVisible = ref(false); const isCreating = ref(false); const saving = ref(false); const error = ref(""); const listError = ref("");
const emptySubAgentForm = (): SubAgentDefinition => ({ id: "", name: "", description: "", builtin: false, inputs: [], outputs: [], system_prompt: "", user_prompt: "", prompt_parts: [], output_mode: "json_ports", llm_kind: "main", progress_message: "", include_graph_tools: false, run_duration: "Short", tool_ids: [] });
const form = reactive<SubAgentDefinition>(emptySubAgentForm());
const columns = [{ colKey: "name", title: "名称", ellipsis: true }, { colKey: "id", title: "ID", width: 180 }, { colKey: "tools", title: "工具", width: 100 }, { colKey: "actions", title: "操作", width: 130 }];
async function load() { loading.value = true; listError.value = ""; try { subagents.value = await system.subagents.list(availableTools.value.map((tool) => tool.id)); } catch (cause) { subagents.value = []; listError.value = cause instanceof Error ? cause.message : String(cause); } finally { loading.value = false; } }
function startCreate() { showTypePicker.value = false; isCreating.value = true; error.value = ""; Object.assign(form, emptySubAgentForm()); editorVisible.value = true; }
async function openEditor(id: string) { error.value = ""; listError.value = ""; try { const definition = await system.subagents.get(id, availableTools.value.map((tool) => tool.id)); // Reset before assigning: a definition that omits an optional text field must not inherit the previous one.
  Object.assign(form, emptySubAgentForm(), definition, { user_prompt: definition.user_prompt ?? "", progress_message: definition.progress_message ?? "" }); isCreating.value = false; editorVisible.value = true; } catch (cause) { listError.value = cause instanceof Error ? cause.message : String(cause); } }
function addPort(kind: "inputs" | "outputs") { form[kind].push({ name: "", data_type: "String", description: "", required: true }); }
function removePort(kind: "inputs" | "outputs", index: number) { form[kind].splice(index, 1); }
async function save() { error.value = ""; saving.value = true; try { // Blank optional text must reach the backend as null: an empty user prompt replaces the default JSON envelope, and an empty progress message would be emitted verbatim.
  const payload: SubAgentDefinition = { ...form, user_prompt: form.user_prompt?.trim() ? form.user_prompt : null, progress_message: form.progress_message?.trim() ? form.progress_message : null }; await system.subagents.save(form.id, payload, availableTools.value.map((tool) => tool.id)); editorVisible.value = false; await load(); } catch (cause) { error.value = cause instanceof Error ? cause.message : String(cause); } finally { saving.value = false; } }
async function remove(id: string) { listError.value = ""; try { await system.subagents.remove(id); await load(); } catch (cause) { listError.value = cause instanceof Error ? cause.message : String(cause); } }
void load();
</script>

<style scoped lang="scss">
.agent-config-hint { margin-bottom: 12px; color: var(--td-text-color-placeholder); font-size: 12px; }.agent-config-alert { margin-top: 12px; }.agent-config-empty { padding: 48px; text-align: center; color: var(--td-text-color-placeholder); }.agent-type-card { display: grid; gap: 8px; padding: 20px; text-align: left; }.agent-type-card span { color: var(--td-text-color-placeholder); font-size: 13px; }.agent-name-cell { display: flex; align-items: center; gap: 8px; min-width: 0; }.agent-name-cell > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }.port-list { display: grid; gap: 8px; width: 100%; }.port-row { display: grid; grid-template-columns: minmax(110px, 1fr) 120px minmax(150px, 2fr) auto auto; align-items: center; gap: 8px; }.agent-error { color: var(--td-error-color); font-size: 12px; } @media (max-width: 720px) { .form-grid, .port-row { grid-template-columns: 1fr; } }
</style>
