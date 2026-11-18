import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { system, isChatModelRef, type ServiceWithRuntime, type LlmConfig } from "../../api/client";
import { planers, type PlanerConfig } from "../../api/planers";

export function usePlanerEdit(planerId?: string) {
  const router = useRouter();
  const items = ref<PlanerConfig[]>([]),
    roles = ref<ServiceWithRuntime[]>([]),
    models = ref<LlmConfig[]>([]);
  const loading = ref(false),
    saving = ref(false),
    error = ref("");
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
      if (planerId) {
        const found = items.value.find(p => p.config_id === planerId);
        form.value = found ? JSON.parse(JSON.stringify(found)) : await planers.get(planerId);
      } else {
        form.value = blank();
      }
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  function selectMembers(value: unknown) {
    form.value.members = (value as string[]).map(
      role_id =>
        form.value.members.find(m => m.role_id === role_id) ??
        { role_id, description: "", workspace_path: null },
    );
  }

  function setAvatar(value: string) {
    form.value.avatar_url = value || null;
  }

  async function save() {
    if (!form.value.name.trim() || !form.value.llm_ref_id) {
      error.value = "请填写名称并选择推理模型";
      return;
    }
    saving.value = true;
    try {
      await planers.save({ ...form.value, avatar_url: form.value.avatar_url || null });
      router.push("/planers");
    } catch (e) {
      error.value = String(e);
    } finally {
      saving.value = false;
    }
  }

  function backToList() {
    router.push("/planers");
  }

  onMounted(load);

  return {
    loading, saving, error, form,
    isChatModelRef, roleLabel, modelName, isWorkspace, otherOwner,
    selectMembers, setAvatar, save, backToList,
  };
}
