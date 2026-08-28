import { createApp } from "vue";
import { createRouter, createWebHistory } from "vue-router";
import TDesignVueNext from "tdesign-vue-next";
import "tdesign-vue-next/dist/tdesign.css";

import { bootstrapGraphEditor } from "./graph_editor_bootstrap";
import AdminApp from "./admin/AdminApp.vue";
import Dashboard from "./admin/view/Dashboard.vue";
import Connections from "./admin/view/Connections.vue";
import ConnectionManager from "./admin/view/ConnectionManager.vue";
import Llm from "./admin/view/Llm.vue";
import AgentConfig from "./admin/view/AgentConfig.vue";
import RoleService from "./admin/view/RoleService.vue";
import Graphs from "./admin/view/Graphs.vue";
import Tasks from "./admin/view/Tasks.vue";
import ScheduledTasks from "./admin/view/ScheduledTasks.vue";
import Logs from "./admin/view/Logs.vue";
import Commands from "./admin/view/Commands.vue";
import Chat from "./admin/view/Chat.vue";
import DataExplorer from "./admin/view/DataExplorer.vue";
import DataExplorerDetail from "./admin/view/DataExplorerDetail.vue";
import Settings from "./admin/view/Settings.vue";
import ApiKeys from "./admin/view/ApiKeys.vue";
import SetupWizard from "./admin/view/SetupWizard.vue";
import Plugins from "./admin/view/Plugins.vue";
import "./admin/admin.scss";
import "./ui/theme.css";
import { initTheme, loadThemes } from "./ui/theme";
import { ws } from "./api/ws";
import { initLogStream } from "./admin/state/logStream";
import { setup as setupApi } from "./api/client";

async function main() {
  if (window.location.pathname.startsWith("/editor")) {
    await bootstrapGraphEditor();
    return;
  }

  initTheme();
  await loadThemes();
  ws.connect();
  initLogStream();

  const router = createRouter({
    history: createWebHistory(),
    routes: [
      { path: "/", component: Dashboard },
      { path: "/connections", component: Connections },
      { path: "/connection-manager", component: ConnectionManager },
      { path: "/llm", component: Llm },
      { path: "/agent-config", component: AgentConfig },
      { path: "/services", component: RoleService },
      { path: "/graphs", component: Graphs },
      { path: "/tasks", component: Tasks },
      { path: "/scheduled-tasks", component: ScheduledTasks },
      { path: "/logs", component: Logs },
      { path: "/commands", component: Commands },
      {
        path: "/chat",
        component: Chat,
        props: (route) => ({
          agentId: typeof route.query.agent_id === "string" ? route.query.agent_id : undefined,
          sessionId: typeof route.query.session_id === "string" ? route.query.session_id : undefined,
        }),
      },
      { path: "/data-explorer", component: DataExplorer },
      { path: "/data-explorer/:serviceId/:capability(messages|memories|images)", component: DataExplorerDetail },
      { path: "/settings", component: Settings },
      { path: "/api-keys", component: ApiKeys },
      { path: "/plugins", component: Plugins },
      { path: "/setup", component: SetupWizard, meta: { public: true } },
      { path: "/setup", component: SetupWizard, meta: { public: true } },
    ],
  });

  router.beforeEach(async (to, _from, next) => {
    if (to.path === "/setup") {
      next();
      return;
    }
    try {
      const status = await setupApi.getStatus();
      if (!status.completed && !status.skipped) {
        next("/setup");
        return;
      }
    } catch {
      // fail open
    }
    next();
  });

  const app = createApp(AdminApp);
  app.use(router);
  app.use(TDesignVueNext);
  app.mount("#app");
}

main().catch((e) => {
  console.error("Fatal startup error:", e);
  alert("应用启动失败，请查看控制台。");
});
