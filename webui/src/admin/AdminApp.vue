<template>
  <t-layout v-if="!isSetupRoute" class="admin-shell" :class="{ 'admin-shell--sidebar-open': sidebarOpen }">
    <t-aside class="admin-aside" :width="sidebarCollapsed ? '56px' : '216px'">
      <div class="admin-brand">
        <img v-if="!sidebarCollapsed" class="admin-brand-logo" :src="brandLogoSrc" alt="Zihuan Next" />
        <span v-if="!sidebarCollapsed" class="admin-brand-title">Zihuan Next</span>
        <t-button class="admin-collapse-toggle" variant="text" shape="square" @click="toggleSidebar">
          <MenuUnfoldIcon v-if="sidebarCollapsed" />
          <MenuFoldIcon v-else />
        </t-button>
      </div>
      <t-menu :theme="menuTheme" :value="activeMenuValue" :collapsed="sidebarCollapsed" class="admin-menu">
        <t-menu-item value="/" to="/" @click="closeSidebar"><template #icon><DashboardIcon /></template>仪表盘</t-menu-item>
        <t-menu-item value="/chat" to="/chat" @click="closeSidebar"><template #icon><ChatIcon /></template>对话</t-menu-item>
        <t-menu-item value="/connections" to="/connections" @click="closeSidebar"><template #icon><LinkIcon /></template>连接配置</t-menu-item>
        <t-menu-item value="/llm" to="/llm" @click="closeSidebar"><template #icon><MindMapIcon /></template>模型配置</t-menu-item>
        <t-menu-item value="/services" to="/services" @click="closeSidebar"><template #icon><ServerIcon /></template>Service 管理</t-menu-item>
        <t-menu-item value="/graphs" to="/graphs" @click="closeSidebar"><template #icon><SitemapIcon /></template>节点图与工作流</t-menu-item>
        <t-menu-item value="/tasks" to="/tasks" @click="closeSidebar"><template #icon><TaskIcon /></template>任务管理器</t-menu-item>
        <t-menu-item value="/scheduled-tasks" to="/scheduled-tasks" @click="closeSidebar"><template #icon><TaskIcon /></template>计划任务</t-menu-item>
        <t-menu-item value="/logs" to="/logs" @click="closeSidebar">
          <template #icon><ArticleIcon /></template>日志
          <t-tag v-if="logErrorBadgeEnabled && errorCount > 0" class="admin-nav-badge" theme="danger" variant="dark" size="small">{{ errorCount }}</t-tag>
        </t-menu-item>
        <t-menu-item value="/commands" to="/commands" @click="closeSidebar"><template #icon><TerminalIcon /></template>命令管理</t-menu-item>
        <t-menu-item value="/connection-manager" to="/connection-manager" @click="closeSidebar"><template #icon><ControlPlatformIcon /></template>连接管理器</t-menu-item>
        <t-menu-item value="/data-explorer" to="/data-explorer" @click="closeSidebar"><template #icon><SearchIcon /></template>数据检索</t-menu-item>
        <t-menu-item value="/api-keys" to="/api-keys" @click="closeSidebar"><template #icon><KeyIcon /></template>API Key</t-menu-item>
        <t-menu-item value="/plugins" to="/plugins" @click="closeSidebar"><template #icon><ExtensionIcon /></template>插件</t-menu-item>
        <t-menu-item value="/settings" to="/settings" @click="closeSidebar"><template #icon><SettingIcon /></template>设置</t-menu-item>
      </t-menu>
    </t-aside>
    <div v-if="showOverlay" class="admin-sidebar-overlay" @click="sidebarOpen = false"></div>
    <t-layout>
      <t-header class="admin-mobile-topbar" height="52px">
        <t-button variant="text" shape="square" @click="toggleSidebar"><MenuUnfoldIcon /></t-button>
        <span class="admin-mobile-brand">Zihuan Next</span>
      </t-header>
      <t-content class="admin-content"><RouterView /></t-content>
    </t-layout>
  </t-layout>
  <div v-else class="setup-fullscreen"><RouterView /></div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { RouterView, useRoute } from "vue-router";
import {
  ArticleIcon, ChatIcon, ControlPlatformIcon, DashboardIcon, ExtensionIcon, KeyIcon, LinkIcon,
  MenuFoldIcon, MenuUnfoldIcon, MindMapIcon, SearchIcon, ServerIcon, SettingIcon, SitemapIcon,
  TaskIcon, TerminalIcon,
} from "tdesign-icons-vue-next";

import brandLogoSrc from "../assets/brand-icon.png";
import { onThemeChange } from "../ui/theme";
import { useAdminApp } from "./composables/useAdminApp";
import { errorCount, logErrorBadgeEnabled } from "./state/logStream";

const { isSetupRoute, sidebarOpen, sidebarCollapsed, showOverlay, closeSidebar, toggleSidebar } = useAdminApp();
const route = useRoute();
const activeMenuValue = computed(() => route.path.startsWith("/data-explorer/") ? "/data-explorer" : route.path);

function readMenuTheme(): "light" | "dark" {
  return document.documentElement.getAttribute("theme-mode") === "light" ? "light" : "dark";
}

const menuTheme = ref(readMenuTheme());
let unsubscribeTheme: (() => void) | undefined;
onMounted(() => { unsubscribeTheme = onThemeChange(() => { menuTheme.value = readMenuTheme(); }); });
onUnmounted(() => { unsubscribeTheme?.(); });
</script>

<style scoped lang="scss">
@use "./styles/admin-app" as *;
</style>
