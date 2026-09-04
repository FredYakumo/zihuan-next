import { onMounted, ref } from "vue";

import {
  graphs as graphApi,
  workflows as workflowApi,
  type GraphTabInfo,
} from "../../api/client";

export function useGraphs() {
  const workflows = ref<
    Array<{
      name: string;
      file: string;
      cover_url: string | null;
      display_name: string | null;
      description: string | null;
      version: string | null;
    }
  >>([]);
  const graphs = ref<GraphTabInfo[]>([]);

  async function load() {
    const [loadedWorkflows, loadedGraphs] = await Promise.all([
      workflowApi.listDetailed(),
      graphApi.list(),
    ]);
    workflows.value = loadedWorkflows.workflows;
    graphs.value = loadedGraphs;
  }

  onMounted(() => {
    load().catch((error) => {
      console.error(error);
      alert(`节点图页面加载失败: ${(error as Error).message}`);
    });
  });

  return {
    workflows,
    graphs,
    load,
  };
}

export type UseGraphsReturn = ReturnType<typeof useGraphs>;
