<template>
  <div class="admin-shell">
    <template v-if="!isSetupRoute">
      <aside class="admin-sidebar">
        <div class="admin-brand">
          <div class="admin-brand-mark">
            <img class="admin-brand-mark-image" :src="brandLogoSrc" alt="Zihuan Next icon" />
          </div>
          <div>
            <h1>Zihuan Next</h1>
            <p>AI框架管理界面</p>
          </div>
        </div>
        <nav class="admin-nav">
          <RouterLink class="admin-nav-link" to="/">仪表盘</RouterLink>
          <RouterLink class="admin-nav-link" to="/connections">连接配置</RouterLink>
          <RouterLink class="admin-nav-link" to="/llm">模型配置</RouterLink>
          <RouterLink class="admin-nav-link" to="/agents">Agent 管理</RouterLink>
          <RouterLink class="admin-nav-link" to="/graphs">节点图与工作流</RouterLink>
          <RouterLink class="admin-nav-link" to="/tasks">任务管理器</RouterLink>
          <RouterLink class="admin-nav-link" to="/commands">命令管理</RouterLink>
          <RouterLink class="admin-nav-link" to="/connection-manager">连接管理器</RouterLink>
          <RouterLink class="admin-nav-link" to="/data-explorer">数据检索</RouterLink>
          <RouterLink class="admin-nav-link" to="/settings">设置</RouterLink>
        </nav>
      </aside>
      <main class="admin-main">
        <RouterView />
      </main>
    </template>
    <template v-else>
      <div class="setup-fullscreen">
        <RouterView />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from "vue";
import { RouterLink, RouterView, useRoute, useRouter } from "vue-router";
import brandLogoSrc from "../assets/brand-icon.png";
import { setup as setupApi } from "../api/client";

const route = useRoute();
const router = useRouter();
const isSetupRoute = computed(() => route.path === "/setup");

onMounted(async () => {
  try {
    const status = await setupApi.getStatus();
    if (!status.completed && !status.skipped && router.currentRoute.value.path !== "/setup") {
      router.push("/setup");
    }
  } catch {
    // fail open
  }
});
</script>

<style scoped>
.setup-fullscreen {
  grid-column: 1 / -1;
  min-height: 100vh;
  background: var(--admin-bg);
}
</style>
