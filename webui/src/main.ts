import { createApp } from "vue";
import { createRouter, createWebHistory } from "vue-router";

import { bootstrapGraphEditor } from "./graph_editor_bootstrap";
import AdminApp from "./admin/AdminApp.vue";
import Dashboard from "./admin/view/Dashboard.vue";
import Connections from "./admin/view/Connections.vue";
import ConnectionManager from "./admin/view/ConnectionManager.vue";
import Llm from "./admin/view/Llm.vue";
import Agents from "./admin/view/Agents.vue";
import Graphs from "./admin/view/Graphs.vue";
import Tasks from "./admin/view/Tasks.vue";
import Commands from "./admin/view/Commands.vue";
import DataExplorer from "./admin/view/DataExplorer.vue";
import Settings from "./admin/view/Settings.vue";
import SetupWizard from "./admin/view/SetupWizard.vue";
import "./admin/admin.scss";
import "./ui/theme.css";
import { initTheme, loadThemes } from "./ui/theme";
import { ws } from "./api/ws";
import { mountLiveLogConsole } from "./ui/live_log_console";
import { setup as setupApi } from "./api/client";

async function main() {
  if (window.location.pathname.startsWith("/editor")) {
    await bootstrapGraphEditor();
    return;
  }

  initTheme();
  await loadThemes();
  ws.connect();
  mountLiveLogConsole();

  const router = createRouter({
    history: createWebHistory(),
    routes: [
      { path: "/", component: Dashboard },
      { path: "/connections", component: Connections },
      { path: "/connection-manager", component: ConnectionManager },
      { path: "/llm", component: Llm },
      { path: "/services", component: Agents },
      { path: "/graphs", component: Graphs },
      { path: "/tasks", component: Tasks },
      { path: "/commands", component: Commands },
      { path: "/data-explorer", component: DataExplorer },
      { path: "/settings", component: Settings },
      { path: "/setup", component: SetupWizard, meta: { public: true } },
    ],
  });

  router.beforeEach(async (to, from, next) => {
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
  app.mount("#app");
}

main().catch((e) => {
  console.error("Fatal startup error:", e);
  alert("应用启动失败，请查看控制台。");
});
