<template>
  <div class="admin-shell">
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
        <RouterLink class="admin-nav-link" to="/connection-manager">连接管理器</RouterLink>
        <RouterLink class="admin-nav-link" to="/data-explorer">数据检索</RouterLink>
        <RouterLink class="admin-nav-link" to="/settings">设置</RouterLink>
      </nav>
    </aside>
    <main class="admin-main">
      <RouterView />
    </main>
  </div>
</template>

<script setup lang="ts">
import { RouterLink, RouterView } from "vue-router";
import brandLogoSrc from "../assets/brand-icon.png";
</script>
