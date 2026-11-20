<template>
  <section class="page">
    <AdminPageHeader title="命令管理" />

    <t-card title="已注册命令" bordered header-bordered class="commands-card">
      <template #actions>
        <t-button variant="text" @click="loadData">刷新</t-button>
      </template>
      <p class="muted">共 {{ commands.length }} 个命令</p>

      <div v-if="commands.length === 0" class="empty-state">正在加载...</div>
      <t-table v-else row-key="name" bordered size="small" :data="commands" :columns="commandColumns">
        <template #name="{ row }">
          <span class="mono">/{{ row.name }}</span>
        </template>
        <template #aliases="{ row }">
          <span v-if="row.aliases.length">
            <code v-for="alias in row.aliases" :key="alias" class="tag">/{{ alias }}</code>
          </span>
          <span v-else class="muted">—</span>
        </template>
        <template #accepted_arg_count="{ row }">
          <code>{{ row.accepted_arg_count ?? 0 }}</code>
        </template>
        <template #scope="{ row }">
          <t-tag variant="light">{{ scopeLabel(row.scope) }}</t-tag>
        </template>
      </t-table>
    </t-card>
  </section>
</template>

<script setup lang="ts">
import AdminPageHeader from "../components/AdminPageHeader.vue";
import { useCommands } from "./useCommands";

const { commands, loadData, commandColumns, scopeLabel } = useCommands();
</script>

<style scoped lang="scss">
@use "./commands" as *;
</style>
