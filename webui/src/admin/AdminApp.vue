<template>
  <div class="admin-shell" :class="{ 'sidebar-open': sidebarOpen, 'sidebar-collapsed': sidebarCollapsed }">
    <template v-if="!isSetupRoute">
      <button
        type="button"
        class="admin-sidebar-toggle admin-sidebar-toggle-floating"
        aria-label="展开菜单"
        @click="toggleSidebar"
      >
        <span class="admin-hamburger"></span>
      </button>
      <aside class="admin-sidebar">
        <div class="admin-brand">
          <div class="admin-brand-mark">
            <img class="admin-brand-mark-image" :src="brandLogoSrc" alt="Zihuan Next icon" />
          </div>
          <div>
            <h1>Zihuan Next</h1>
          </div>
          <button
            type="button"
            class="admin-sidebar-toggle"
            aria-label="切换菜单"
            @click="toggleSidebar"
          >
            <span class="admin-hamburger"></span>
          </button>
        </div>
        <nav class="admin-nav">
          <RouterLink class="admin-nav-link" to="/" @click="closeSidebar">仪表盘</RouterLink>
          <RouterLink class="admin-nav-link" to="/chat" @click="closeSidebar">对话</RouterLink>
          <RouterLink class="admin-nav-link" to="/connections" @click="closeSidebar">连接配置</RouterLink>
          <RouterLink class="admin-nav-link" to="/llm" @click="closeSidebar">模型配置</RouterLink>
          <RouterLink class="admin-nav-link" to="/services" @click="closeSidebar">Service 管理</RouterLink>
          <RouterLink class="admin-nav-link" to="/graphs" @click="closeSidebar">节点图与工作流</RouterLink>
          <RouterLink class="admin-nav-link" to="/tasks" @click="closeSidebar">任务管理器</RouterLink>
          <RouterLink class="admin-nav-link" to="/logs" @click="closeSidebar">
            日志
            <span v-if="logErrorBadgeEnabled && errorCount > 0" class="admin-nav-badge">{{ errorCount }}</span>
          </RouterLink>
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
            @click="toggleSidebar"
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
import { RouterLink, RouterView } from "vue-router";
import brandLogoSrc from "../assets/brand-icon.png";
import { useAdminApp } from "./composables/useAdminApp";
import { errorCount, logErrorBadgeEnabled } from "./state/logStream";

const { isSetupRoute, sidebarOpen, sidebarCollapsed, showOverlay, closeSidebar, toggleSidebar } = useAdminApp();
</script>

<style scoped lang="scss">
@use "./styles/admin-app" as *;
</style>
