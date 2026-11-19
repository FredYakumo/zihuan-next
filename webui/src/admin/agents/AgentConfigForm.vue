<template>
  <t-form label-align="top">
    <div class="form-grid"><t-form-item label="ID" required><t-input v-model="form.id" :disabled="!isCreating" placeholder="例如 research_agent" /></t-form-item><t-form-item label="名称" required><t-input v-model="form.name" /></t-form-item></div>
    <t-form-item label="描述"><t-input v-model="form.description" placeholder="提供给 LLM 的工具描述，留空则使用名称" /></t-form-item>
    <div class="form-grid"><t-form-item label="模型用途"><t-input v-model="form.llm_kind" placeholder="main / math_programming" /></t-form-item><t-form-item label="运行时长"><t-select v-model="form.run_duration"><t-option value="Short" label="Short" /><t-option value="Long" label="Long" /></t-select></t-form-item></div>
    <div class="form-grid"><t-form-item label="输出模式"><t-select v-model="form.output_mode"><t-option value="json_ports" label="JSON 输出端口" /><t-option value="text" label="纯文本" /></t-select></t-form-item><t-form-item label="工具进度提示"><t-input v-model="form.progress_message" placeholder="运行前写入 dashboard 的进度文本" /></t-form-item></div>
    <t-form-item label="输入"><div class="port-list"><div v-for="(port, index) in form.inputs" :key="`input-${index}`" class="port-row"><t-input v-model="port.name" placeholder="字段名" /><t-select v-model="port.data_type"><t-option v-for="type in portTypes" :key="type" :value="type" :label="type" /></t-select><t-input v-model="port.description" placeholder="描述" /><t-checkbox v-model="port.required">必填</t-checkbox><t-button variant="text" theme="danger" @click="removePort('inputs', index)">删除</t-button></div><t-button variant="dashed" size="small" @click="addPort('inputs')">新增输入</t-button></div></t-form-item>
    <t-form-item label="系统提示词"><t-textarea v-model="form.system_prompt" :autosize="{ minRows: 5, maxRows: 12 }" /></t-form-item>
    <t-form-item label="用户提示词模板"><t-textarea v-model="form.user_prompt" :autosize="{ minRows: 3, maxRows: 10 }" placeholder="使用 {输入名} 占位；留空则使用默认 JSON 封装" /></t-form-item>
    <t-form-item label="输出"><div class="port-list"><div v-for="(port, index) in form.outputs" :key="`output-${index}`" class="port-row"><t-input v-model="port.name" placeholder="字段名" /><t-select v-model="port.data_type"><t-option v-for="type in portTypes" :key="type" :value="type" :label="type" /></t-select><t-input v-model="port.description" placeholder="描述" /><t-checkbox v-model="port.required">必填</t-checkbox><t-button variant="text" theme="danger" @click="removePort('outputs', index)">删除</t-button></div><t-button variant="dashed" size="small" @click="addPort('outputs')">新增输出</t-button></div></t-form-item>
    <t-form-item label="工具列表"><t-checkbox-group v-model="form.tool_ids"><t-checkbox v-for="tool in availableTools" :key="tool.id" :value="tool.id">{{ tool.name }}</t-checkbox></t-checkbox-group></t-form-item><t-form-item label="节点图工具"><t-checkbox v-model="form.include_graph_tools">合并角色定义的节点图工具</t-checkbox></t-form-item><div v-if="error" class="agent-error">{{ error }}</div>
  </t-form>
</template>

<script lang="ts">
import { defineComponent, type PropType } from "vue";
import type { UseAgentConfigReturn } from "./AgentConfig";

export default defineComponent({
  name: "AgentConfigForm",
  props: {
    page: { type: Object as PropType<UseAgentConfigReturn>, required: true },
  },
  setup(props) {
    return { ...props.page };
  },
});
</script>

<style scoped lang="scss">
@use "./AgentConfig" as *;
</style>
