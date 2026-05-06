<template>
  <section class="page">
    <div class="page-hero">
      <h2>节点图与工作流</h2>
      <div class="hero-actions connection-hero-actions">
        <a class="btn primary" href="/editor">打开编辑器</a>
        <a class="btn" href="/editor">新建空白节点图</a>
      </div>
    </div>

    <div class="grid-2">
      <section class="panel">
        <div class="split-header">
          <div>
            <h3>工作流集</h3>
          </div>
          <button class="btn ghost" @click="load">刷新</button>
        </div>
        <div class="list" style="margin-top: 12px;">
          <div v-if="workflows.length === 0" class="empty-state">还没有工作流集。</div>
          <article v-for="workflow in workflows" :key="workflow.file" class="record">
            <div class="split-header">
              <div>
                <h4>{{ workflow.display_name || workflow.name }}</h4>
                <div class="record-meta">
                  <span>{{ workflow.file }}</span>
                  <span v-if="workflow.version">v{{ workflow.version }}</span>
                </div>
              </div>
              <a class="btn" :href="`/editor?workflow=${encodeURIComponent(workflow.name)}`">在编辑器打开</a>
            </div>
            <p v-if="workflow.description" class="muted" style="margin-top: 10px;">{{ workflow.description }}</p>
          </article>
        </div>
      </section>

      <section class="panel">
        <div class="split-header">
          <div>
            <h3>当前图会话</h3>
          </div>
        </div>
        <div class="list" style="margin-top: 12px;">
          <div v-if="graphs.length === 0" class="empty-state">当前没有图会话。</div>
          <article v-for="graph in graphs" :key="graph.id" class="record">
            <h4>{{ graph.name }}</h4>
            <div class="record-meta">
              <span>{{ graph.node_count }} nodes</span>
              <span>{{ graph.edge_count }} edges</span>
              <span>{{ graph.file_path || "未保存到文件" }}</span>
            </div>
          </article>
        </div>
      </section>
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";

import { graphs as graphApi, workflows as workflowApi, type GraphTabInfo } from "../../api/client";

const workflows = ref<Array<{ name: string; file: string; cover_url: string | null; display_name: string | null; description: string | null; version: string | null }>>([]);
const graphs = ref<GraphTabInfo[]>([]);

async function load() {
  const [loadedWorkflows, loadedGraphs] = await Promise.all([
    workflowApi.listDetailed(),
    graphApi.list(),
  ]);
  workflows.value = loadedWorkflows.workflows;
  graphs.value = loadedGraphs;
}

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`节点图页面加载失败: ${(error as Error).message}`);
  });
});
</script>
