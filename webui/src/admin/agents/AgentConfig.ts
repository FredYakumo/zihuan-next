import { computed, reactive, ref } from "vue";

import { registry, system, type SubAgentDefinition } from "../../api/client";
import type { NodeToolInfo } from "../../api/types";
import { QQ_CHAT_DEFAULT_TOOLS, WORKSPACE_DEFAULT_TOOLS } from "../model";

const portTypes = ["String", "Integer", "Float", "Boolean", "Json"];
// Callable tools that the role-service default tool lists do not cover because they are configured
// through their own switches; a definition may still reference them.
const SWITCH_CONFIGURED_TOOLS = [{ id: "image_understand", name: "image_understand" }];

export function useAgentConfig() {
  const subagents = ref<SubAgentDefinition[]>([]);
  const nodeTools = ref<NodeToolInfo[]>([]);
  const loading = ref(false);
  const showTypePicker = ref(false);
  const editorVisible = ref(false);
  const isCreating = ref(false);
  const saving = ref(false);
  const error = ref("");
  const listError = ref("");

  const availableTools = computed(() => {
    const defaults = [
      ...QQ_CHAT_DEFAULT_TOOLS.map((tool) => ({ id: tool.id, name: tool.label })),
      ...WORKSPACE_DEFAULT_TOOLS.filter(
        (tool) => !QQ_CHAT_DEFAULT_TOOLS.some((item) => item.id === tool.id),
      ).map((tool) => ({ id: tool.id, name: tool.label })),
      ...SWITCH_CONFIGURED_TOOLS.filter(
        (tool) =>
          !QQ_CHAT_DEFAULT_TOOLS.some((item) => item.id === tool.id) &&
          !WORKSPACE_DEFAULT_TOOLS.some((item) => item.id === tool.id),
      ),
    ];
    // Existing sub-agents are callable tools too, so a definition can reference another.
    const known = new Set(defaults.map((tool) => tool.id));
    const nested = subagents.value
      .filter((agent) => !known.has(agent.id))
      .map((agent) => ({ id: agent.id, name: agent.name }));
    // A registered DAG node is callable as a tool on its own, so a definition may name one.
    const nodes = nodeTools.value
      .filter((tool) => !known.has(tool.id) && !nested.some((item) => item.id === tool.id))
      .map((tool) => ({ id: tool.id, name: `${tool.name}（节点）` }));
    return [...defaults, ...nested, ...nodes];
  });

  const emptySubAgentForm = (): SubAgentDefinition => ({
    id: "",
    name: "",
    description: "",
    builtin: false,
    inputs: [],
    outputs: [],
    system_prompt: "",
    user_prompt: "",
    prompt_parts: [],
    output_mode: "json_ports",
    llm_kind: "main",
    progress_message: "",
    include_graph_tools: false,
    run_duration: "Short",
    tool_ids: [],
  });
  const form = reactive<SubAgentDefinition>(emptySubAgentForm());
  const columns = [
    { colKey: "name", title: "名称", ellipsis: true },
    { colKey: "id", title: "ID", width: 180 },
    { colKey: "tools", title: "工具", width: 100 },
    { colKey: "actions", title: "操作", width: 130 },
  ];

  async function load() {
    loading.value = true;
    listError.value = "";
    try {
      const [definitions, nodes] = await Promise.all([
        system.subagents.list(availableTools.value.map((tool) => tool.id)),
        registry.getNodeTools().catch(() => []),
      ]);
      subagents.value = definitions;
      nodeTools.value = nodes;
    } catch (cause) {
      subagents.value = [];
      listError.value = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading.value = false;
    }
  }

  function startCreate() {
    showTypePicker.value = false;
    isCreating.value = true;
    error.value = "";
    Object.assign(form, emptySubAgentForm());
    editorVisible.value = true;
  }

  async function openEditor(id: string) {
    error.value = "";
    listError.value = "";
    try {
      const definition = await system.subagents.get(
        id,
        availableTools.value.map((tool) => tool.id),
      );
      // Reset before assigning: a definition that omits an optional text field must not inherit the previous one.
      Object.assign(form, emptySubAgentForm(), definition, {
        user_prompt: definition.user_prompt ?? "",
        progress_message: definition.progress_message ?? "",
      });
      isCreating.value = false;
      editorVisible.value = true;
    } catch (cause) {
      listError.value = cause instanceof Error ? cause.message : String(cause);
    }
  }

  function addPort(kind: "inputs" | "outputs") {
    form[kind].push({ name: "", data_type: "String", description: "", required: true });
  }

  function removePort(kind: "inputs" | "outputs", index: number) {
    form[kind].splice(index, 1);
  }

  async function save() {
    error.value = "";
    saving.value = true;
    try {
      // Blank optional text must reach the backend as null: an empty user prompt replaces the default JSON envelope, and an empty progress message would be emitted verbatim.
      const payload: SubAgentDefinition = {
        ...form,
        user_prompt: form.user_prompt?.trim() ? form.user_prompt : null,
        progress_message: form.progress_message?.trim() ? form.progress_message : null,
      };
      await system.subagents.save(
        form.id,
        payload,
        availableTools.value.map((tool) => tool.id),
      );
      editorVisible.value = false;
      await load();
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving.value = false;
    }
  }

  async function remove(id: string) {
    listError.value = "";
    try {
      await system.subagents.remove(id);
      await load();
    } catch (cause) {
      listError.value = cause instanceof Error ? cause.message : String(cause);
    }
  }

  void load();

  return {
    portTypes,
    availableTools,
    subagents,
    loading,
    showTypePicker,
    editorVisible,
    isCreating,
    saving,
    error,
    listError,
    form,
    columns,
    load,
    startCreate,
    openEditor,
    addPort,
    removePort,
    save,
    remove,
  };
}

export type UseAgentConfigReturn = ReturnType<typeof useAgentConfig>;
