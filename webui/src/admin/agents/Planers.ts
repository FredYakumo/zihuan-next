import { onMounted, ref } from "vue";
import { system, isChatModelRef, type ServiceWithRuntime, type LlmConfig } from "../../api/client";
import { planers, type PlanerConfig } from "../../api/planers";

export function usePlaners() {
  const items = ref<PlanerConfig[]>([]),
    roles = ref<ServiceWithRuntime[]>([]),
    models = ref<LlmConfig[]>([]);
  const loading = ref(false),
    saving = ref(false),
    visible = ref(false),
    error = ref("");
  const chatAgent = ref<ServiceWithRuntime | null>(null);
  const form = ref(blank());

  function blank(): PlanerConfig {
    return {
      config_id: "",
      name: "",
      enabled: true,
      auto_start: false,
      avatar_url: null,
      llm_ref_id: "",
      prompt: "",
      updated_at: "",
      members: [],
    };
  }

  function roleLabel(id: string) {
    const role = roles.value.find(r => r.config_id === id);
    return role ? `${role.name} (${role.role_service_type.type})` : `${id}（已失效）`;
  }

  function modelName(id: string) {
    return models.value.find(m => m.config_id === id)?.name || id;
  }

  function isWorkspace(id: string) {
    return roles.value.find(r => r.config_id === id)?.role_service_type.type === "workspace";
  }

  function otherOwner(id: string) {
    return items.value.find(
      p => p.config_id !== form.value.config_id && p.members.some(m => m.role_id === id),
    )?.name;
  }

  async function load() {
    loading.value = true;
    try {
      [items.value, roles.value, models.value] = await Promise.all([
        planers.list(),
        system.services.list(),
        system.llm.list(),
      ]);
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  function edit(item?: PlanerConfig) {
    error.value = "";
    form.value = item ? JSON.parse(JSON.stringify(item)) : blank();
    visible.value = true;
  }

  function selectMembers(value: unknown) {
    form.value.members = (value as string[]).map(
      role_id =>
        form.value.members.find(m => m.role_id === role_id) ??
        { role_id, description: "", workspace_path: null },
    );
  }

  async function save() {
    if (!form.value.name.trim() || !form.value.llm_ref_id) {
      error.value = "请填写名称并选择推理模型";
      return;
    }
    saving.value = true;
    try {
      await planers.save(form.value);
      visible.value = false;
      await load();
    } catch (e) {
      error.value = String(e);
    } finally {
      saving.value = false;
    }
  }

  async function lifecycle(item: PlanerConfig) {
    try {
      await (item.runtime?.status === "running"
        ? planers.stop(item.config_id)
        : planers.start(item.config_id));
      await load();
    } catch (e) {
      error.value = String(e);
    }
  }

  async function remove(item: PlanerConfig) {
    try {
      await planers.remove(item.config_id);
      await load();
    } catch (e) {
      error.value = String(e);
    }
  }

  onMounted(load);

  return {
    items, roles, models, loading, saving, visible, error, chatAgent, form,
    isChatModelRef, roleLabel, modelName, isWorkspace, otherOwner,
    load, edit, selectMembers, save, lifecycle, remove,
  };
}
