import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { system, type ServiceWithRuntime, type LlmConfig } from "../../api/client";
import { planers, planerChatTarget, type PlanerConfig } from "../../api/planers";

export function usePlaners() {
  const router = useRouter();
  const items = ref<PlanerConfig[]>([]),
    roles = ref<ServiceWithRuntime[]>([]),
    models = ref<LlmConfig[]>([]);
  const loading = ref(false),
    error = ref("");
  const chatAgent = ref<ServiceWithRuntime | null>(null);

  function roleLabel(id: string) {
    const role = roles.value.find(r => r.config_id === id);
    return role ? `${role.name} (${role.role_service_type.type})` : `${id}（已失效）`;
  }

  function modelName(id: string) {
    return models.value.find(m => m.config_id === id)?.name || id;
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

  function create() {
    router.push("/planers/edit");
  }

  function edit(item: PlanerConfig) {
    router.push({ path: "/planers/edit", query: { id: item.config_id } });
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
    items, roles, models, loading, error, chatAgent,
    roleLabel, modelName, load, create, edit, lifecycle, remove,
  };
}
