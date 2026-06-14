<template>
  <div class="admin-shell" :class="{ 'sidebar-open': sidebarOpen }">
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
          <button
            type="button"
            class="admin-sidebar-toggle"
            aria-label="切换菜单"
            @click="sidebarOpen = !sidebarOpen"
          >
            <span class="admin-hamburger"></span>
          </button>
        </div>
        <nav class="admin-nav">
          <RouterLink class="admin-nav-link" to="/" @click="closeSidebar">仪表盘</RouterLink>
          <RouterLink class="admin-nav-link" to="/connections" @click="closeSidebar">连接配置</RouterLink>
          <RouterLink class="admin-nav-link" to="/llm" @click="closeSidebar">模型配置</RouterLink>
          <RouterLink class="admin-nav-link" to="/services" @click="closeSidebar">Service 管理</RouterLink>
          <RouterLink class="admin-nav-link" to="/graphs" @click="closeSidebar">节点图与工作流</RouterLink>
          <RouterLink class="admin-nav-link" to="/tasks" @click="closeSidebar">任务管理器</RouterLink>
          <RouterLink class="admin-nav-link" to="/commands" @click="closeSidebar">命令管理</RouterLink>
          <RouterLink class="admin-nav-link" to="/connection-manager" @click="closeSidebar">连接管理器</RouterLink>
          <RouterLink class="admin-nav-link" to="/data-explorer" @click="closeSidebar">数据检索</RouterLink>
          <RouterLink class="admin-nav-link" to="/settings" @click="closeSidebar">设置</RouterLink>
        </nav>
      </aside>
      <div v-if="showOverlay" class="admin-sidebar-overlay" @click="sidebarOpen = false"></div>
      <main class="admin-main">
        <header class="admin-mobile-topbar">
          <button
            type="button"
            class="admin-sidebar-toggle"
            aria-label="切换菜单"
            @click="sidebarOpen = !sidebarOpen"
          >
            <span class="admin-hamburger"></span>
          </button>
          <span class="admin-mobile-brand">Zihuan Next</span>
        </header>
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
import { computed, onMounted, ref } from "vue";
import { RouterLink, RouterView, useRoute, useRouter } from "vue-router";
import brandLogoSrc from "../assets/brand-icon.png";
import { setup as setupApi } from "../api/client";

const route = useRoute();
const router = useRouter();
const isSetupRoute = computed(() => route.path === "/setup");
const sidebarOpen = ref(false);

const showOverlay = computed(() => {
  return sidebarOpen.value && window.matchMedia("(max-width: 900px)").matches;
});

function closeSidebar() {
  sidebarOpen.value = false;
}

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
