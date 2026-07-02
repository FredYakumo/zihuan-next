import { computed, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";

import {
  system,
  type LlmConfig,
  type NotificationCard,
  type ServiceWithRuntime,
} from "../../api/client";
import {
  agentAvatarUrl,
  agentInitial,
  compactId,
  statusTone,
  CHAT_ELIGIBLE_SERVICE_TYPES,
} from "../model";


export function useDashboard() {
  const router = useRouter();

  const services = ref<ServiceWithRuntime[]>([]);
  const servicesLoading = ref(false);
  const llmModels = ref<LlmConfig[]>([]);
  const operatingId = ref("");
  const pendingAction = ref<"start" | "stop" | "">("");
  const chatModalAgentId = ref("");
  const chatModalSessionId = ref("");
  const notificationCards = ref<Array<NotificationCard & { agentName: string }>>([]);
  const selectedNotificationCard = ref<(NotificationCard & { agentName: string }) | null>(
    null,
  );
  const clearingNotifications = ref(false);

  const stats = reactive({
    connections: 0,
    llm: 0,
    agents: 0,
  });

  const chatModalService = computed(() =>
    services.value.find((service) => service.config_id === chatModalAgentId.value),
  );

  function llmName(service: ServiceWithRuntime): string {
    const agentType = service.agent_type as Record<string, unknown>;
    const llmId = String(agentType.llm_ref_id ?? "");
    if (!llmId) {
      return "未绑定";
    }
    return llmModels.value.find((item) => item.config_id === llmId)?.name ?? "未知";
  }

  function runtimeBadgeText(service: ServiceWithRuntime): string {
    switch (service.runtime.status) {
      case "running":
        return service.runtime.instance_id
          ? `已启动 (${compactId(service.runtime.instance_id)})`
          : "已启动";
      case "stopped":
        return "已停止";
      case "starting":
        return "启动中";
      case "error":
        return "启动失败";
      default:
        return service.runtime.status;
    }
  }

  async function startService(id: string) {
    if (operatingId.value) {
      return;
    }
    operatingId.value = id;
    pendingAction.value = "start";
    try {
      await system.services.start(id);
      await load();
    } catch (error) {
      alert(`启动失败: ${(error as Error).message}`);
    } finally {
      operatingId.value = "";
      pendingAction.value = "";
    }
  }

  async function stopService(id: string) {
    if (operatingId.value) {
      return;
    }
    operatingId.value = id;
    pendingAction.value = "stop";
    try {
      await system.services.stop(id);
      await load();
    } catch (error) {
      alert(`停止失败: ${(error as Error).message}`);
    } finally {
      operatingId.value = "";
      pendingAction.value = "";
    }
  }

  function openChatModal(agentId: string) {
    chatModalAgentId.value = agentId;
    chatModalSessionId.value = "";
  }

  function openNotificationKeyModal(card: NotificationCard & { agentName: string }) {
    selectedNotificationCard.value = card;
  }

  async function clearAllNotifications() {
    if (clearingNotifications.value) {
      return;
    }
    clearingNotifications.value = true;
    try {
      const qqServices = services.value.filter((item) => item.agent_type.type === "qq_chat");
      await Promise.all(
        qqServices.map((service) =>
          system.services.deleteNotifications(service.config_id)
        ),
      );
      notificationCards.value = [];
    } catch (error) {
      alert(`清空失败: ${(error as Error).message}`);
    } finally {
      clearingNotifications.value = false;
    }
  }

  function closeChatModal() {
    chatModalAgentId.value = "";
    chatModalSessionId.value = "";
  }

  function openChatInNewWindow() {
    if (!chatModalAgentId.value) {
      return;
    }
    const query: Record<string, string> = { agent_id: chatModalAgentId.value };
    if (chatModalSessionId.value) {
      query.session_id = chatModalSessionId.value;
    }
    const routeUrl = router.resolve({ path: "/chat", query });
    window.open(routeUrl.href, "_blank");
  }

  async function load() {
    servicesLoading.value = true;
    try {
      const [connections, llm, loadedAgents] = await Promise.all([
        system.connections.list(),
        system.llm.list(),
        system.services.list(),
      ]);
      stats.connections = connections.length;
      stats.llm = llm.length;
      stats.agents = loadedAgents.length;
      services.value = loadedAgents;
      llmModels.value = llm;
      const qqServices = loadedAgents.filter((item) => item.agent_type.type === "qq_chat");
      const cardGroups = await Promise.all(
        qqServices.map(async (service) => {
          const cards = await system.services.listNotifications(service.config_id);
          return cards.map((card) => ({ ...card, agentName: service.name }));
        }),
      );
      notificationCards.value = cardGroups
        .flat()
        .sort((a, b) => b.created_at.localeCompare(a.created_at));
    } finally {
      servicesLoading.value = false;
    }
  }

  onMounted(() => {
    load().catch((error) => {
      console.error(error);
      alert(`仪表盘加载失败: ${(error as Error).message}`);
    });
  });

  return {
    services,
    servicesLoading,
    llmModels,
    operatingId,
    pendingAction,
    chatModalAgentId,
    chatModalSessionId,
    notificationCards,
    selectedNotificationCard,
    clearingNotifications,
    stats,
    chatModalService,
    llmName,
    runtimeBadgeText,
    startService,
    stopService,
    openChatModal,
    openNotificationKeyModal,
    clearAllNotifications,
    closeChatModal,
    openChatInNewWindow,
    load,
    compactId,
    agentAvatarUrl,
    agentInitial,
    statusTone,
    CHAT_ELIGIBLE_SERVICE_TYPES,
  };
}

export type UseDashboardReturn = ReturnType<typeof useDashboard>;
