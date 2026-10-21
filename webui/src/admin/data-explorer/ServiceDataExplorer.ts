import { useRouter } from "vue-router";
import type { PrimaryTableCol } from "tdesign-vue-next";
import type { ServiceWithRuntime } from "../../api/client";
import { serviceCapabilities, type ServiceCapability, useDataExplorerList } from "./useDataExplorer";

export function useServiceDataExplorer() {
  const router = useRouter();
  const { explorerServices } = useDataExplorerList();
  const columns: PrimaryTableCol<ServiceWithRuntime>[] = [{ colKey: "name", title: "Service" }, { colKey: "type", title: "类型", width: 150 }, { colKey: "status", title: "状态", width: 120 }, { colKey: "actions", title: "操作", width: 340 }];

  function serviceTypeLabel(type: string) { return type === "qq_chat" ? "QQ Chat" : type === "http_stream" ? "HTTP Stream" : "Workspace"; }
  function capabilityLabel(capability: ServiceCapability) { return capability === "messages" ? "聊天记录" : capability === "memories" ? "记忆" : capability === "images" ? "图片库" : "上下文"; }
  function openDetail(serviceId: string, capability: ServiceCapability) { void router.push(`/data-explorer/${encodeURIComponent(serviceId)}/${capability}`); }
  function returnToEntry() { void router.push("/data-explorer"); }

  return { explorerServices, columns, serviceTypeLabel, capabilityLabel, openDetail, returnToEntry, serviceCapabilities };
}
