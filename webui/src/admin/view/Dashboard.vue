<template>
  <section class="page">
    <div class="page-hero">
      <h2>服务运行总览</h2>
      <div class="hero-actions" style="margin-top: 12px;">
        <a class="btn primary" href="/editor">进入节点图编辑器</a>
        <RouterLink class="btn" to="/agents">管理 Agent</RouterLink>
        <RouterLink class="btn" to="/connections">管理连接</RouterLink>
      </div>
    </div>

    <div class="stat-grid">
      <article class="stat-card">
        <span class="muted">连接配置</span>
        <strong>{{ stats.connections }}</strong>
      </article>
      <article class="stat-card">
        <span class="muted">模型配置</span>
        <strong>{{ stats.llm }}</strong>
      </article>
      <article class="stat-card">
        <span class="muted">Agent</span>
        <strong>{{ stats.agents }}</strong>
      </article>
    </div>

    <div class="grid-2">
      <section class="panel">
        <div class="split-header">
          <div>
            <h3>Agent 运行状态</h3>
          </div>
          <RouterLink class="btn ghost" to="/agents">全部管理</RouterLink>
        </div>
        <div class="list" style="margin-top: 12px;">
          <div v-if="agents.length === 0" class="empty-state">还没有创建 Agent。</div>
          <article v-for="agent in agents.slice(0, 4)" :key="agent.id" class="record">
            <div class="split-header">
              <div>
                <h4>{{ agent.name }}</h4>
                <div class="record-meta">
                  <span>{{ readableAgentType(agent.agent_type.type) }}</span>
                  <span v-if="agent.is_default">默认入口</span>
                  <span>{{ agent.enabled ? "已启用" : "已禁用" }}</span>
                </div>
              </div>
              <span class="badge" :class="statusTone(agent.runtime.status)">{{ agent.runtime.status }}</span>
            </div>
            <p v-if="agent.runtime.last_error" class="muted" style="margin-top: 10px;">{{ agent.runtime.last_error }}</p>
          </article>
        </div>
      </section>

      <section class="panel">
        <div class="split-header">
          <div>
            <h3>最近任务</h3>
          </div>
          <RouterLink class="btn ghost" to="/tasks">查看全部</RouterLink>
        </div>
        <div class="list" style="margin-top: 12px;">
          <div v-if="tasks.length === 0" class="empty-state">最近还没有任务记录。</div>
          <article v-for="task in tasks.slice(0, 5)" :key="task.id" class="record">
            <div class="split-header">
              <div>
                <h4>{{ task.graph_name }}</h4>
                <div class="record-meta">
                  <span>{{ formatTime(task.start_time) }}</span>
                  <span v-if="task.user_ip">IP {{ task.user_ip }}</span>
                </div>
              </div>
              <span class="badge" :class="statusTone(task.status)">{{ task.status }}</span>
            </div>
          </article>
        </div>
      </section>
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { RouterLink } from "vue-router";

import { system, tasks as taskApi, type AgentWithRuntime, type TaskEntry } from "../../api/client";
import { formatTime, statusTone } from "../model";

const agents = ref<AgentWithRuntime[]>([]);
const tasks = ref<TaskEntry[]>([]);
const stats = reactive({
  connections: 0,
  llm: 0,
  agents: 0,
});

function readableAgentType(type: string): string {
  return type === "http_stream" ? "HTTP Stream Agent" : "QQ Chat Agent";
}

async function load() {
  const [connections, llm, loadedAgents, loadedTasks] = await Promise.all([
    system.connections.list(),
    system.llm.list(),
    system.agents.list(),
    taskApi.list(),
  ]);
  stats.connections = connections.length;
  stats.llm = llm.length;
  stats.agents = loadedAgents.length;
  agents.value = loadedAgents;
  tasks.value = loadedTasks;
}

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`仪表盘加载失败: ${(error as Error).message}`);
  });
});
</script>
