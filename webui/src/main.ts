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
import DataExplorer from "./admin/view/DataExplorer.vue";
import Settings from "./admin/view/Settings.vue";
import "./admin/admin.scss";
import "./ui/theme.css";
import { initTheme, loadThemes } from "./ui/theme";
import { ws } from "./api/ws";
import { mountLiveLogConsole } from "./ui/live_log_console";

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
      { path: "/agents", component: Agents },
      { path: "/graphs", component: Graphs },
      { path: "/tasks", component: Tasks },
      { path: "/data-explorer", component: DataExplorer },
      { path: "/settings", component: Settings },
    ],
  });

  const app = createApp(AdminApp);
  app.use(router);
  app.mount("#app");
}

main().catch((e) => {
  console.error("Fatal startup error:", e);
  alert("应用启动失败，请查看控制台。");
});
