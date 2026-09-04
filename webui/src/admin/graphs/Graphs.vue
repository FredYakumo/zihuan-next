<template>
  <section class="page">
    <AdminPageHeader title="节点图与工作流">
      <t-button tag="a" href="/editor" theme="primary">打开编辑器</t-button>
      <t-button tag="a" href="/editor" variant="outline">新建空白节点图</t-button>
    </AdminPageHeader>

    <div class="grid-2">
      <t-card title="工作流集" bordered header-bordered>
        <template #actions>
          <t-button variant="text" @click="load">刷新</t-button>
        </template>
        <div v-if="workflows.length === 0" class="empty-state">还没有工作流集。</div>
        <div v-else class="list">
          <article v-for="workflow in workflows" :key="workflow.file" class="record">
            <div class="split-header">
              <div>
                <h4>{{ workflow.display_name || workflow.name }}</h4>
                <div class="record-meta">
                  <span>{{ workflow.file }}</span>
                  <span v-if="workflow.version">v{{ workflow.version }}</span>
                </div>
              </div>
              <t-button tag="a" :href="`/editor?workflow=${encodeURIComponent(workflow.name)}`" variant="outline" size="small">
                在编辑器打开
              </t-button>
            </div>
            <p v-if="workflow.description" class="muted" style="margin-top: 10px;">{{ workflow.description }}</p>
          </article>
        </div>
      </t-card>

      <t-card title="当前图会话" bordered header-bordered>
        <div v-if="graphs.length === 0" class="empty-state">当前没有图会话。</div>
        <div v-else class="list">
          <article v-for="graph in graphs" :key="graph.id" class="record">
            <h4>{{ graph.name }}</h4>
            <div class="record-meta">
              <span>{{ graph.node_count }} nodes</span>
              <span>{{ graph.edge_count }} edges</span>
              <span>{{ graph.file_path || "未保存到文件" }}</span>
            </div>
          </article>
        </div>
      </t-card>
    </div>
  </section>
</template>
<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useGraphs } from "./useGraphs";

const { workflows, graphs, load } = useGraphs();
</script>
