<template>
  <t-layout v-if="!isSetupRoute" class="admin-shell" :class="{ 'admin-shell--sidebar-open': sidebarOpen }">
    <t-aside class="admin-aside" :width="sidebarCollapsed ? '56px' : '216px'">
      <div class="admin-brand">
        <img class="admin-brand-logo" :src="brandLogoSrc" alt="Zihuan Next" />
        <span v-if="!sidebarCollapsed" class="admin-brand-title">Zihuan Next</span>
      </div>
      <t-menu :theme="menuTheme" :value="activeMenuValue" :collapsed="sidebarCollapsed" class="admin-menu">
        <t-menu-item value="/" to="/" @click="closeSidebar">仪表盘</t-menu-item>
        <t-menu-item value="/chat" to="/chat" @click="closeSidebar">对话</t-menu-item>
        <t-menu-item value="/connections" to="/connections" @click="closeSidebar">连接配置</t-menu-item>
        <t-menu-item value="/llm" to="/llm" @click="closeSidebar">模型配置</t-menu-item>
        <t-menu-item value="/services" to="/services" @click="closeSidebar">Service 管理</t-menu-item>
        <t-menu-item value="/graphs" to="/graphs" @click="closeSidebar">节点图与工作流</t-menu-item>
        <t-menu-item value="/tasks" to="/tasks" @click="closeSidebar">任务管理器</t-menu-item>
        <t-menu-item value="/logs" to="/logs" @click="closeSidebar">
          日志
          <t-tag v-if="logErrorBadgeEnabled && errorCount > 0" class="admin-nav-badge" theme="danger" variant="dark" size="small">
            {{ errorCount }}
          </t-tag>
        </t-menu-item>
        <t-menu-item value="/commands" to="/commands" @click="closeSidebar">命令管理</t-menu-item>
        <t-menu-item value="/connection-manager" to="/connection-manager" @click="closeSidebar">连接管理器</t-menu-item>
        <t-menu-item value="/data-explorer" to="/data-explorer" @click="closeSidebar">数据检索</t-menu-item>
        <t-menu-item value="/settings" to="/settings" @click="closeSidebar">设置</t-menu-item>
      </t-menu>
      <t-button class="admin-collapse-toggle" variant="text" shape="square" @click="toggleSidebar">
        <MenuUnfoldIcon v-if="sidebarCollapsed" />
        <MenuFoldIcon v-else />
      </t-button>
    </t-aside>
    <div v-if="showOverlay" class="admin-sidebar-overlay" @click="sidebarOpen = false"></div>
    <t-layout>
      <t-header class="admin-mobile-topbar" height="52px">
        <t-button variant="text" shape="square" @click="toggleSidebar">
          <MenuUnfoldIcon />
        </t-button>
        <span class="admin-mobile-brand">Zihuan Next</span>
      </t-header>
      <t-content class="admin-content">
        <RouterView />
      </t-content>
    </t-layout>
  </t-layout>
  <div v-else class="setup-fullscreen">
    <RouterView />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { RouterView, useRoute } from "vue-router";
import { MenuFoldIcon, MenuUnfoldIcon } from "tdesign-icons-vue-next";
import brandLogoSrc from "../assets/brand-icon.png";
import { useAdminApp } from "./composables/useAdminApp";
import { errorCount, logErrorBadgeEnabled } from "./state/logStream";
import { onThemeChange } from "../ui/theme";

const { isSetupRoute, sidebarOpen, sidebarCollapsed, showOverlay, closeSidebar, toggleSidebar } = useAdminApp();

const route = useRoute();
const activeMenuValue = computed(() => route.path);

function readMenuTheme(): "light" | "dark" {
  return document.documentElement.getAttribute("theme-mode") === "light" ? "light" : "dark";
}

const menuTheme = ref(readMenuTheme());
let unsubscribeTheme: (() => void) | undefined;

onMounted(() => {
  unsubscribeTheme = onThemeChange(() => {
    menuTheme.value = readMenuTheme();
  });
});

onUnmounted(() => {
  unsubscribeTheme?.();
});
</script>

<style scoped lang="scss">
@use "./styles/admin-app" as *;
</style>
