<template>
    <section class="page agent-service-page">
        <AdminPageHeader title="Service 管理">
            <t-button variant="outline" @click="triggerServiceImportFile">导入配置</t-button>
            <input ref="serviceImportFileInput" type="file" accept=".json" class="agent-service-import-input"
                @change="handleServiceFileChange" />
            <t-button theme="primary" @click="startCreate">新建 Service</t-button>
        </AdminPageHeader>


        <t-drawer v-model:visible="showCreatePicker" size="960px" :close-on-overlay-click="false" :footer="false"
            @close="closeCreatePicker">
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
                <AgentServiceForm :page="page" />

                <div class="agent-service-drawer-footer">
                    <t-button variant="outline" @click="showCreateForm = false">返回</t-button>
                    <t-button theme="primary" @click="submitForm">创建 Service</t-button>
                </div>
            </div>

            <div v-else class="agent-service-type-grid">
                <t-button v-for="type in serviceTypes" :key="type.value" variant="outline"
                    class="agent-service-type-card" @click="pickCreateType(type.value)">
                    <div class="agent-service-type-card-content">
                        <strong>{{ type.label }}</strong>
                        <span class="agent-service-type-hint">{{ type.hint }}</span>
                    </div>
                </t-button>
            </div>
        </t-drawer>
        <t-drawer v-model:visible="showEditModal" :header="form.name || '编辑 Service'" size="960px" :close-btn="true"
            :close-on-overlay-click="false" @close="closeEditModal">
            <AgentServiceForm :page="page" />

            <template #footer>
                <div class="agent-service-drawer-footer">
                    <t-button variant="outline" @click="closeEditModal">取消</t-button>
                    <t-button theme="primary" @click="submitForm">保存</t-button>
                </div>
            </template>
        </t-drawer>


        <t-drawer v-model:visible="showDefaultToolEditModal" :header="'编辑默认工具'" size="480px"
            :close-on-overlay-click="false" @close="closeDefaultToolEditModal">
            <t-form class="agent-service-form" label-align="top">
                <t-card class="agent-service-form-section" :bordered="false">
                    <t-form-item label="工具">
                        <div class="agent-service-form-hint">{{ currentEditingDefaultTool?.label }} ({{
                            currentEditingDefaultTool?.id }})</div>
                    </t-form-item>
                    <t-checkbox v-model="defaultToolEditDraft.enabled">启用该工具</t-checkbox>
                    <t-form-item label="单次会话调用上限" style="margin-top: 16px">
                        <t-input-number v-model="defaultToolEditDraft.callLimit" :min="0" placeholder="不限制" />
                        <div class="agent-service-form-hint" style="font-size: 12px; margin-top: 4px">0 或留空表示不限制</div>
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


        <t-drawer v-model:visible="showToolEditModal" size="560px" :close-on-overlay-click="false"
            @close="closeToolEditModal">
            <template #header>
                <div class="agent-service-tool-drawer-header">
                    <t-tooltip v-if="showToolTypeBack" content="返回选择工具类型">
                        <t-button variant="text" shape="square" aria-label="返回" @click="backToToolTypePicker">
                            <ChevronLeftIcon />
                        </t-button>
                    </t-tooltip>
                    <strong>{{ editingToolIndex === -1 ? '增加工具' : '编辑工具' }}</strong>
                </div>
            </template>
            <div v-if="editingToolIndex === -1 && toolCreateStep === 'picker'" class="agent-service-tool-type-picker">
                <p class="agent-service-tool-type-title">选择工具类型</p>
                <div class="agent-service-tool-type-grid">
                    <button v-for="item in toolTypeOptions" :key="`${item.value}-${item.scriptLanguage ?? ''}`"
                        type="button" class="agent-service-tool-type-card" @click="selectToolType(item)">
                        <component :is="item.icon" class="agent-service-tool-type-icon" />
                        <strong class="agent-service-tool-type-name">{{ item.label }}</strong>
                        <span class="agent-service-tool-type-desc">{{ item.desc }}</span>
                    </button>
                </div>
            </div>
            <t-form v-else class="agent-service-form" label-align="top">
                <t-card class="agent-service-form-section" :bordered="false">
                    <div class="agent-service-form-grid">
                        <t-form-item v-if="toolEditDraft.implementation === 'plugin'" label="插件" required
                            class="agent-service-form-item-full">
                            <div class="agent-service-plugin-picker">
                                <t-input v-model="brainToolPluginFilter" placeholder="搜索插件名称、工具名或描述"
                                    clearable>
                                    <template #prefix-icon><SearchIcon /></template>
                                </t-input>
                                <div v-if="brainToolPluginsLoading" class="agent-service-form-hint">正在加载已安装插件...
                                </div>
                                <div v-else-if="filteredBrainToolPlugins.length === 0" class="agent-service-form-hint">
                                    {{ brainToolPlugins.length === 0
                                        ? '还没有可用的插件工具，请先在「插件」页安装 brain_tool 类型的插件。'
                                        : '没有匹配的插件。' }}
                                </div>
                                <div v-else class="agent-service-plugin-list">
                                    <button v-for="plugin in filteredBrainToolPlugins"
                                        :key="plugin.name" type="button"
                                        :class="['agent-service-plugin-option', { active: toolEditDraft.pluginName === plugin.name }]"
                                        @click="selectBrainToolPlugin(plugin)">
                                        <div class="agent-service-plugin-option-head">
                                            <strong>{{ plugin.display_name }}</strong>
                                            <code>{{ plugin.tool_name }}</code>
                                            <t-tag v-if="toolEditDraft.pluginName === plugin.name"
                                                size="small" variant="light" theme="success">已选</t-tag>
                                        </div>
                                        <p v-if="plugin.description" class="agent-service-plugin-option-desc">
                                            {{ plugin.description }}</p>
                                        <p v-else-if="plugin.tool_description"
                                            class="agent-service-plugin-option-desc">{{ plugin.tool_description }}</p>
                                        <div class="agent-service-plugin-option-meta">
                                            <span>v{{ plugin.version || "0" }}</span>
                                            <span>{{ plugin.language }}</span>
                                        </div>
                                    </button>
                                </div>
                            </div>
                        </t-form-item>
                        <template v-if="toolEditDraft.implementation === 'plugin' && selectedBrainToolPlugin">
                            <t-form-item label="工具名称" class="agent-service-form-item-full">
                                <t-input v-model="toolEditDraft.name" />
                                <div class="agent-service-form-hint">
                                    工具名由插件声明，一般不需要改；改了会影响模型看到的工具名。
                                </div>
                            </t-form-item>
                            <t-form-item label="工具描述" class="agent-service-form-item-full">
                                <t-input v-model="toolEditDraft.description" />
                            </t-form-item>
                            <t-form-item label="工具参数（插件声明，只读）" class="agent-service-form-item-full">
                                <pre class="agent-service-plugin-params">{{ JSON.stringify(selectedBrainToolPlugin.parameters, null, 2) }}</pre>
                            </t-form-item>
                        </template>
                        <t-form-item v-if="toolEditDraft.implementation === 'plugin'" style="align-self: end">
                            <t-checkbox v-model="toolEditDraft.enabled">启用该工具</t-checkbox>
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'sub_agent'" label="Sub-agent" required>
                            <t-select v-model="toolEditDraft.subAgentId" placeholder="请选择"
                                @change="selectSubAgent(String($event))">
                                <t-option v-for="agent in subAgents" :key="agent.id" :value="agent.id"
                                    :label="agent.name" />
                            </t-select>
                            <div v-if="subAgents.length === 0" class="agent-service-form-hint">还没有可用的 Sub-agent，请先在
                                Agent 配置中创建。
                            </div>
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'sub_agent'" style="align-self: end">
                            <t-checkbox v-model="toolEditDraft.enabled">启用该工具</t-checkbox>
                        </t-form-item>
                        <div v-if="toolEditDraft.implementation === 'sub_agent' && selectedSubAgent"
                            class="agent-service-subagent-preview agent-service-form-item-full">
                            <div class="agent-service-subagent-preview-head">
                                <strong>{{ selectedSubAgent.name }}</strong>
                                <code>{{ selectedSubAgent.id }}</code>
                                <t-tag v-if="selectedSubAgent.builtin" size="small" variant="light"
                                    theme="success">Built-in</t-tag>
                            </div>
                            <p v-if="selectedSubAgent.description" class="agent-service-subagent-preview-desc">{{
                                selectedSubAgent.description }}</p>
                            <dl class="agent-service-subagent-preview-meta">
                                <div>
                                    <dt>运行时长</dt>
                                    <dd>{{ selectedSubAgent.run_duration }}</dd>
                                </div>
                                <div>
                                    <dt>输出模式</dt>
                                    <dd>{{ subAgentOutputModeLabel }}</dd>
                                </div>
                                <div>
                                    <dt>模型用途</dt>
                                    <dd>{{ selectedSubAgent.llm_kind }}</dd>
                                </div>
                            </dl>
                            <div class="agent-service-subagent-preview-ports">
                                <div class="agent-service-subagent-preview-port-group">
                                    <span class="agent-service-subagent-preview-port-title">输入</span>
                                    <div v-if="selectedSubAgent.inputs.length === 0" class="agent-service-form-hint">无
                                    </div>
                                    <div v-for="port in selectedSubAgent.inputs" :key="`in-${port.name}`"
                                        class="agent-service-subagent-preview-port">
                                        <code>{{ port.name }}</code>
                                        <span class="agent-service-subagent-preview-port-type">{{
                                            formatDataType(port.data_type)
                                            }}</span>
                                        <t-tag v-if="port.required" size="small" variant="light"
                                            theme="warning">必填</t-tag>
                                        <span v-if="port.description"
                                            class="agent-service-subagent-preview-port-desc">{{
                                            port.description }}</span>
                                    </div>
                                </div>
                                <div class="agent-service-subagent-preview-port-group">
                                    <span class="agent-service-subagent-preview-port-title">输出</span>
                                    <div v-if="selectedSubAgent.outputs.length === 0" class="agent-service-form-hint">无
                                    </div>
                                    <div v-for="port in selectedSubAgent.outputs" :key="`out-${port.name}`"
                                        class="agent-service-subagent-preview-port">
                                        <code>{{ port.name }}</code>
                                        <span class="agent-service-subagent-preview-port-type">{{
                                            formatDataType(port.data_type)
                                            }}</span>
                                        <t-tag v-if="port.required" size="small" variant="light"
                                            theme="warning">必填</t-tag>
                                        <span v-if="port.description"
                                            class="agent-service-subagent-preview-port-desc">{{
                                            port.description }}</span>
                                    </div>
                                </div>
                            </div>
                            <div v-if="selectedSubAgent.system_prompt" class="agent-service-subagent-preview-prompt">
                                <span class="agent-service-subagent-preview-port-title">System Prompt</span>
                                <pre class="agent-service-subagent-preview-prompt-text">{{ selectedSubAgent.system_prompt }}
                        </pre>
                            </div>
                            <div class="agent-service-subagent-preview-tools">
                                <span class="agent-service-subagent-preview-port-title">可调用工具（{{
                                    selectedSubAgent.tool_ids.length }}）</span>
                                <div v-if="selectedSubAgent.tool_ids.length === 0" class="agent-service-form-hint">无
                                </div>
                                <div v-else class="agent-service-subagent-preview-tool-tags">
                                    <t-tag v-for="toolId in selectedSubAgent.tool_ids" :key="toolId" size="small"
                                        variant="outline">{{ toolId }}</t-tag>
                                </div>
                            </div>
                        </div>
                        <t-form-item v-if="toolEditDraft.implementation === 'node_graph'" label="名称">
                            <t-input v-model="toolEditDraft.name" />
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'node_graph'" label="描述"
                            class="agent-service-form-item-full">
                            <t-input v-model="toolEditDraft.description" />
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'node_graph'" label="运行时长">
                            <t-select v-model="toolEditDraft.runDuration">
                                <t-option value="Short" label="Short（短时）" />
                                <t-option value="Long" label="Long（长时）" />
                            </t-select>
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'node_graph'" label="目标类型">
                            <t-select v-model="toolEditDraft.targetType"
                                @change="handleToolTargetTypeChange(toolEditDraft)">
                                <t-option value="workflow_set" label="workflow_set" />
                                <t-option value="file_path" label="file_path" />
                                <t-option value="inline_graph" label="inline_graph" />
                            </t-select>
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'node_graph'" style="align-self: end">
                            <t-checkbox v-model="toolEditDraft.enabled">启用该工具</t-checkbox>
                        </t-form-item>
                        <t-form-item v-if="form.type === 'qq_chat' && toolEditDraft.enabled" label="单次会话调用上限">
                            <t-input-number v-model="toolEditCallLimit" :min="0" placeholder="不限制" />
                            <div class="agent-service-form-hint" style="font-size: 12px">0 或留空表示不限制</div>
                        </t-form-item>
                        <t-form-item
                            v-if="toolEditDraft.implementation === 'node_graph' && toolEditDraft.targetType === 'workflow_set'"
                            label="Workflow Set 名称" class="agent-service-form-item-full">
                            <t-select v-model="toolEditDraft.workflowName"
                                @change="applyWorkflowSetMetadata(toolEditDraft)" placeholder="请选择">
                                <t-option v-for="workflow in workflows" :key="workflow.name" :value="workflow.name"
                                    :label="workflow.display_name || workflow.name" />
                            </t-select>
                        </t-form-item>
                        <t-form-item
                            v-else-if="toolEditDraft.implementation === 'node_graph' && toolEditDraft.targetType === 'file_path'"
                            label="文件路径" class="agent-service-form-item-full">
                            <t-input v-model="toolEditDraft.filePath" placeholder="workflow_set/demo.json" />
                        </t-form-item>
                        <t-form-item v-else-if="toolEditDraft.implementation === 'node_graph'" label="Inline Graph JSON"
                            class="agent-service-form-item-full">
                            <t-textarea v-model="toolEditDraft.inlineGraphJson" />
                        </t-form-item>
                        <!-- The label slot is the label: TDesign prefers the `label` prop over the slot, so
                 passing both would silently drop the action button next to the title. -->
                        <t-form-item v-if="toolEditDraft.implementation === 'node_graph'"
                            class="agent-service-form-item-full agent-service-label-action">
                            <template #label>
                                <div class="agent-service-params-label">
                                    <span>Parameters JSON</span>
                                    <t-button
                                        v-if="toolEditDraft.targetType === 'workflow_set' && toolEditDraft.workflowName"
                                        variant="text" size="small" :disabled="syncingToolIndex === editingToolIndex"
                                        @click="syncToolFromGraph(toolEditDraft, editingToolIndex)">
                                        {{ syncingToolIndex === editingToolIndex ? '同步中…' : '从节点图更新' }}
                                    </t-button>
                                </div>
                            </template>
                            <t-textarea v-model="toolEditDraft.parametersJson" />
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'node_graph'" label="Outputs JSON"
                            class="agent-service-form-item-full">
                            <t-textarea v-model="toolEditDraft.outputsJson" />
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'script'" label="名称">
                            <t-input v-model="toolEditDraft.name" />
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'script'" label="脚本语言">
                            <t-select v-model="toolEditDraft.scriptLanguage" @change="handleScriptLanguageChanged">
                                <t-option value="type_script" label="TypeScript (.ts)" />
                                <t-option value="python" label="Python (.py)" />
                            </t-select>
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'script'" label="描述"
                            class="agent-service-form-item-full">
                            <t-input v-model="toolEditDraft.description" placeholder="告诉模型这个工具做什么" />
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'script'" label="运行时长">
                            <t-select v-model="toolEditDraft.runDuration">
                                <t-option value="Short" label="Short（短时）" />
                                <t-option value="Long" label="Long（长时）" />
                            </t-select>
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'script'" label="超时（秒）">
                            <t-input-number v-model="toolEditDraft.scriptTimeoutSecs" :min="1" />
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'script'" label="入口函数">
                            <t-input v-model="toolEditDraft.scriptEntry" placeholder="run_tool"
                                @change="handleScriptEntryEdited" />
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'script'" style="align-self: end">
                            <t-checkbox v-model="toolEditDraft.enabled">启用该工具</t-checkbox>
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'script'"
                            class="agent-service-form-item-full agent-service-script-source-item agent-service-label-action">
                            <template #label>
                                <div class="agent-service-params-label">
                                    <span>脚本内容</span>
                                    <span class="agent-service-script-toolbar">
                                        <span v-if="scriptSourceFileName" class="agent-service-script-file-name">{{
                                            scriptSourceFileName }}</span>
                                        <t-button variant="outline" size="small"
                                            @click="openScriptFilePicker">上传脚本文件</t-button>
                                    </span>
                                </div>
                            </template>
                            <t-textarea v-model="toolEditDraft.scriptSource" class="agent-service-script-editor"
                                :autosize="{ minRows: 14, maxRows: 30 }" :placeholder="scriptEditorPlaceholder"
                                @change="handleScriptSourceEdited" />
                            <input ref="scriptUploadInput" type="file" class="agent-service-script-file-input"
                                :accept="scriptFileAccept(toolEditDraft.scriptLanguage)"
                                @change="handleScriptFileSelected" />
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'script'"
                            class="agent-service-form-item-full agent-service-label-action">
                            <template #label>
                                <div class="agent-service-params-label">
                                    <span>Parameters JSON</span>
                                    <t-button variant="text" size="small" :disabled="syncingScriptManifest"
                                        @click="syncToolManifestFromScript">
                                        {{ syncingScriptManifest ? '读取中…' : '从脚本读取' }}
                                    </t-button>
                                </div>
                            </template>
                            <t-textarea v-model="toolEditDraft.parametersJson" class="agent-service-script-json"
                                :autosize="{ minRows: 5, maxRows: 16 }" />
                        </t-form-item>
                        <t-form-item v-if="toolEditDraft.implementation === 'script'" label="Outputs JSON"
                            class="agent-service-form-item-full">
                            <t-textarea v-model="toolEditDraft.outputsJson" class="agent-service-script-json"
                                :autosize="{ minRows: 5, maxRows: 16 }" />
                        </t-form-item>
                    </div>
                </t-card>
            </t-form>
            <template #footer>
                <div v-if="editingToolIndex !== -1 || toolCreateStep === 'config'" class="agent-service-drawer-footer">
                    <t-button variant="outline" @click="closeToolEditModal">取消</t-button>
                    <t-button theme="primary" @click="confirmToolEdit">保存</t-button>
                </div>
            </template>
        </t-drawer>


        <t-drawer v-model:visible="showEmotionDimensionsModal" header="情绪维度" size="640px"
            :close-on-overlay-click="false" :footer="false" @close="closeEmotionDimensionsModal">
            <t-card class="agent-service-form-section" :bordered="false">
                <template #title>
                    <div class="agent-service-section-title-row">
                        <span>维度列表</span>
                        <t-button variant="text" :disabled="emotionDimensionAdding"
                            @click="startAddEmotionDimension">新增维度</t-button>
                    </div>
                </template>
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
                            <t-input v-model="emotionDimensionDraft.positive_prompt"
                                placeholder="维度值正向时的语言风格，留空用维度名称" />
                        </t-form-item>
                        <t-form-item label="负向风格提示词（可选）" class="agent-service-form-item-full">
                            <t-input v-model="emotionDimensionDraft.negative_prompt"
                                placeholder="维度值负向时的语言风格，留空用「不+维度名称」" />
                        </t-form-item>
                    </div>
                    <div class="agent-service-tool-actions" style="margin-top: 12px">
                        <t-button variant="outline" @click="cancelAddEmotionDimension">取消</t-button>
                        <t-button theme="primary" @click="confirmAddEmotionDimension">新增</t-button>
                    </div>
                </t-card>

                <div v-if="!emotionDimensionAdding && form.emotion_dimensions.length === 0"
                    class="agent-service-empty-state">
                    还没有配置情绪维度。点击「新增维度」开始添加。</div>

                <t-card v-for="(dimension, index) in form.emotion_dimensions" :key="`${dimension.name}-${index}`"
                    :bordered="true" style="margin-top: 12px">
                    <template #title>
                        <div v-if="emotionDimensionEditingIndex === index" class="agent-service-tool-header">
                            <strong>编辑维度</strong>
                            <t-button variant="text" @click="cancelEditEmotionDimension">取消</t-button>
                        </div>
                        <div v-else class="agent-service-tool-header">
                            <strong>{{ dimension.name }}</strong>
                            <div>
                                <t-button variant="text" size="small"
                                    :disabled="emotionDimensionAdding || emotionDimensionEditingIndex != null"
                                    @click="editEmotionDimension(index)">编辑</t-button>
                                <t-button variant="text" theme="danger" size="small"
                                    :disabled="emotionDimensionAdding || emotionDimensionEditingIndex != null"
                                    @click="removeEmotionDimension(index)">删除</t-button>
                            </div>
                        </div>
                    </template>
                    <template v-if="emotionDimensionEditingIndex === index">
                        <div class="agent-service-form-grid">
                            <t-form-item label="名称">
                                <t-input v-model="emotionDimensionDraft.name" placeholder="例如：开心" />
                            </t-form-item>
                            <t-form-item label="升权重 (0–20)">
                                <t-slider v-model="emotionDimensionDraft.increase_weight" :min="0" :max="20"
                                    :step="0.1" />
                            </t-form-item>
                            <t-form-item label="降权重 (0–20)">
                                <t-slider v-model="emotionDimensionDraft.decrease_weight" :min="0" :max="20"
                                    :step="0.1" />
                            </t-form-item>
                            <t-form-item label="消解时间（小时）">
                                <t-input-number v-model="emotionDimensionDraft.dissipation_hours" :min="1" :step="1" />
                            </t-form-item>
                            <t-form-item label="正向风格提示词（可选）" class="agent-service-form-item-full">
                                <t-input v-model="emotionDimensionDraft.positive_prompt"
                                    placeholder="维度值正向时的语言风格提示，留空用维度名称" />
                            </t-form-item>
                            <t-form-item label="负向风格提示词（可选）" class="agent-service-form-item-full">
                                <t-input v-model="emotionDimensionDraft.negative_prompt"
                                    placeholder="维度值负向时的语言风格提示，留空用「不+维度名称」" />
                            </t-form-item>
                        </div>
                        <div class="agent-service-tool-actions" style="margin-top: 12px">
                            <t-button variant="outline" @click="cancelEditEmotionDimension">取消</t-button>
                            <t-button theme="primary" @click="confirmEditEmotionDimension">保存</t-button>
                        </div>
                    </template>


                    <template v-else>
                        <div class="agent-service-emotion-bars">
                            <div class="agent-service-emotion-bar-row">
                                <span class="agent-service-emotion-bar-label">升权重</span>
                                <div class="agent-service-emotion-bar-track">
                                    <div class="agent-service-emotion-bar-fill agent-service-emotion-bar-fill--increase"
                                        :style="{ width: Math.min(((dimension.increase_weight ?? 1) / 20) * 100, 100) + '%' }" />
                                </div>
                                <span class="agent-service-emotion-bar-value">{{ dimension.increase_weight ?? 1
                                    }}</span>
                            </div>
                            <div class="agent-service-emotion-bar-row">
                                <span class="agent-service-emotion-bar-label">降权重</span>
                                <div class="agent-service-emotion-bar-track">
                                    <div class="agent-service-emotion-bar-fill agent-service-emotion-bar-fill--decrease"
                                        :style="{ width: Math.min(((dimension.decrease_weight ?? 1) / 20) * 100, 100) + '%' }" />
                                </div>
                                <span class="agent-service-emotion-bar-value">{{ dimension.decrease_weight ?? 1
                                    }}</span>
                            </div>
                        </div>
                        <div class="agent-service-form-hint" style="margin-top: 8px">无对话 {{ dimension.dissipation_hours
                            ?? 5 }} 小时后自动恢复默认</div>
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


        <t-drawer v-model:visible="showIgnoreRulesModal" header="Ignore Rules" size="760px" :close-btn="true"
            :close-on-overlay-click="false" :footer="false" @close="closeIgnoreRulesModal">
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
                <div v-if="ignoreRuleError" class="agent-service-form-hint"
                    style="color: var(--td-error-color); margin-top: 12px">{{ ignoreRuleError }}</div>
                <div style="display: flex; gap: 8px; margin-top: 12px">
                    <t-button variant="outline" :disabled="ignoreRuleSubmitting"
                        @click="resetIgnoreRuleForm">清空</t-button>
                    <t-button theme="primary" :disabled="ignoreRuleSubmitting" @click="submitIgnoreRule">
                        {{ ignoreRuleSubmitting ? (ignoreRuleForm.id == null ? '新增中…' : '保存中…') : (ignoreRuleForm.id ==
                        null ?
                        '新增' : '保存') }}
                    </t-button>
                </div>
            </t-card>

            <IgnoreRulesList :rules="ignoreRules" :loading="ignoreRulesLoading" :submitting="ignoreRuleSubmitting"
                :deleting-id="ignoreRuleDeletingId" :format-rule="formatIgnoreRule" @edit="editIgnoreRule"
                @remove="removeIgnoreRule" />
        </t-drawer>


        <RateLimitConfigDrawer v-model:visible="showRateLimitModal" :form="form" />

        <t-dialog v-model:visible="avatarEditorVisible" header="编辑头像" width="420px" :close-on-overlay-click="false"
            :confirm-btn="{ content: '使用此头像', loading: avatarUploading }" cancel-btn="取消" @confirm="confirmAvatarCrop"
            @cancel="closeAvatarEditor" @close="closeAvatarEditor">
            <div class="agent-service-avatar-editor">
                <div class="agent-service-avatar-editor-preview-row">
                    <div class="agent-service-avatar-preview-column">
                        <t-button variant="outline" shape="circle" class="agent-service-avatar-rotate-handle"
                            :disabled="!avatarDraftUrl" aria-label="进入自由旋转模式" title="按住并拖动以旋转头像"
                            @pointerdown="startAvatarRotation">
                            <RotateIcon />
                        </t-button>
                        <div :key="avatarDraftUrl || 'avatar-placeholder'" class="agent-service-avatar-crop-window"
                            @pointerdown.capture="startAvatarCrop" @pointermove="moveAvatarCrop"
                            @pointerup="endAvatarCrop" @pointercancel="endAvatarCrop"
                            @lostpointercapture="endAvatarCrop" @dragstart.prevent>
                            <img v-if="avatarDraftUrl" :key="avatarDraftUrl" :src="avatarDraftUrl" alt="头像裁剪预览"
                                :draggable="false" class="agent-service-avatar-crop-image"
                                :style="{ transform: `translate(${avatarPan.x}px, ${avatarPan.y}px) scale(${avatarZoom}) rotate(${avatarRotation}deg)` }" />
                            <div v-else class="agent-service-avatar-placeholder">{{ form.name ? form.name.slice(0,
                                1).toUpperCase() : 'A' }}</div>
                        </div>
                    </div>
                    <div v-if="avatarDraftUrl" class="agent-service-avatar-vertical-zoom">
                        <div class="agent-service-avatar-zoom-ruler" aria-hidden="true">
                            <span v-for="tick in avatarZoomRulerTicks" :key="tick"
                                class="agent-service-avatar-zoom-tick"
                                :class="{ 'agent-service-avatar-zoom-tick--major': tick % 1 === 0 }" />
                        </div>
                        <t-slider v-model="avatarZoom" :min="1" :max="3" :step="0.05" layout="vertical" />
                    </div>
                </div>
                <div class="agent-service-avatar-editor-actions">
                    <input ref="avatarEditorFileInput" type="file" accept="image/*"
                        class="agent-service-script-file-input" @change="handleAvatarFileSelect" />
                    <t-button variant="outline" @click="$refs.avatarEditorFileInput?.click()">选择图片</t-button>
                    <t-button variant="text" shape="square" :disabled="!avatarDraftUrl" title="还原图片默认"
                        aria-label="还原图片默认" @click="resetAvatarTransform">
                        <RefreshIcon />
                    </t-button>
                    <t-button variant="text" theme="danger" @click="clearAvatar">使用默认头像</t-button>
                </div>
                <div class="agent-service-form-hint">拖动头像可移动位置；按住头像上方的旋转图标并拖动可自由旋转。</div>
            </div>
        </t-dialog>


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

            <t-table row-key="config_id" :data="filteredServices" :columns="columns" :loading="servicesLoading"
                :hover="true" :pagination="false" table-layout="fixed">
                <template #name="{ row }">
                    <div class="agent-service-name-cell">
                        <img v-if="agentAvatarUrl(row)" :src="agentAvatarUrl(row)" class="agent-service-avatar"
                            alt="" />
                        <span v-else class="agent-service-avatar agent-service-avatar--fallback">{{
                            agentInitial(row.name) }}</span>
                        <div><strong>{{ row.name }}</strong><small class="mono">{{ compactId(row.config_id) }}</small>
                        </div>
                    </div>
                </template>
                <template #type="{ row }"><t-tag variant="light">{{ serviceTypeLabel(row.role_service_type.type)
                        }}</t-tag></template>
                <template #model="{ row }"><span :title="llmName(row)">{{ llmName(row) }}</span></template>
                <template #runtime="{ row }"><t-tag :theme="runtimeTheme(row.runtime.status)" variant="light">{{
                        runtimeBadgeText(row) }}</t-tag></template>
                <template #enabled="{ row }"><t-tag :theme="row.enabled ? 'success' : 'default'" variant="light">{{
                    row.enabled ? '已启用' : '已停用' }}</t-tag></template>
                <template #updated="{ row }"><span>{{ formatTime(row.runtime.started_at) }}</span></template>
                <template #actions="{ row }">
                    <div class="agent-service-actions">
                        <t-button variant="text" size="small" @click="editService(row)">编辑</t-button>
                        <t-button variant="text" size="small" @click="duplicateService(row)">复制添加</t-button>
                        <t-button variant="text" size="small" @click="copyServiceConfigItem(row)">{{ serviceCopiedId ===
                            row.config_id ? '已复制' : '复制' }}</t-button>
                        <t-button variant="text" :theme="row.runtime.status === 'running' ? 'warning' : 'primary'"
                            size="small" @click="toggleServiceRuntime(row)">{{ row.runtime.status === 'running' ? '停止' :
                            '启动' }}</t-button>
                        <t-popconfirm content="确认删除这个 Service 吗？" @confirm="removeService(row.config_id)"><t-button
                                variant="text" theme="danger" size="small">删除</t-button></t-popconfirm>
                    </div>
                </template>
                <template #empty>
                    <div class="agent-service-empty">暂无匹配的 Service。</div>
                </template>
            </t-table>
        </t-card>
        <ConfigImportDialog v-model:visible="showModelConfigDialog" title="新增模型配置" create-label="新增模型配置"
            :loading="modelImporting" @create="openModelCreatePage" @clipboard-import="importModelFromClipboard"
            @file-change="handleModelFileChange" />
        <RetrievalDatabaseDialog v-model:visible="showRetrievalDatabaseDialog" @created="handleRetrievalDatabaseCreated"
            @create-page="openRetrievalDatabaseCreatePage" @clipboard-import="importRetrievalDatabaseFromClipboard"
            @file-change="handleRetrievalDatabaseFileChange" />
        <ConfigImportDialog v-model:visible="showWebSearchDialog" title="新增 Web Search" create-label="新增 Web Search"
            :loading="webSearchImporting" @create="openWebSearchCreatePage"
            @clipboard-import="importWebSearchFromClipboard" @file-change="handleWebSearchFileChange" />
    </section>
</template>

<script lang="ts">
import { defineComponent } from "vue";
import { ChevronLeftIcon, CloseIcon, RefreshIcon, RotateIcon, SearchIcon } from "tdesign-icons-vue-next";
import AdminPageHeader from "../components/AdminPageHeader.vue";
import AgentServiceForm from "../components/AgentServiceForm.vue";
import ConfigImportDialog from "../components/ConfigImportDialog.vue";
import IgnoreRulesList from "../components/IgnoreRulesList.vue";
import RateLimitConfigDrawer from "../components/RateLimitConfigDrawer.vue";
import RetrievalDatabaseDialog from "../components/RetrievalDatabaseDialog.vue";
import { useRoleServicePage } from "./roleServicePage";

export default defineComponent({
    components: {
        AdminPageHeader,
        AgentServiceForm,
        ChevronLeftIcon,
        CloseIcon,
        ConfigImportDialog,
        IgnoreRulesList,
        RateLimitConfigDrawer,
        RefreshIcon,
        RotateIcon,
        SearchIcon,
        RetrievalDatabaseDialog,
    },
    setup() {
        const page = useRoleServicePage();
        // `page` feeds the shared AgentServiceForm; the spread keeps every existing
        // top-level template binding of the drawers and the service table working.
        return { ...page, page };
    },
});
</script>

<style scoped lang="scss">
@use "./role-service" as *;
</style>
