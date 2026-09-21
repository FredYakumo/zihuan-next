<template>
  <t-card class="ignore-rules-list" :bordered="false">
    <template #title>现有规则</template>
    <div v-if="loading" class="ignore-rules-list__empty">加载中...</div>
    <div v-else-if="rules.length === 0" class="ignore-rules-list__empty">还没有规则。</div>
    <t-card v-for="rule in rules" :key="rule.id" :bordered="true" class="ignore-rules-list__card">
      <template #title>
        <div class="ignore-rules-list__header">
          <strong>#{{ rule.id }}</strong>
          <div>
            <t-button variant="text" size="small" :disabled="submitting || deletingId === rule.id" @click="emit('edit', rule)">编辑</t-button>
            <t-button variant="text" theme="danger" size="small" :disabled="submitting || deletingId === rule.id" @click="emit('remove', rule.id)">
              {{ deletingId === rule.id ? '删除中…' : '删除' }}
            </t-button>
          </div>
        </div>
      </template>
      <div class="ignore-rules-list__field"><strong>sender_id</strong><span>{{ rule.sender_id || '未设置' }}</span></div>
      <div class="ignore-rules-list__field"><strong>group_id</strong><span>{{ rule.group_id || '未设置' }}</span></div>
      <div class="ignore-rules-list__field"><strong>含义</strong><span>{{ formatRule(rule.sender_id, rule.group_id) }}</span></div>
    </t-card>
  </t-card>
</template>

<script setup lang="ts">
import type { QqChatAgentServiceIgnoreRule } from "../../api/client";

defineProps<{
  rules: QqChatAgentServiceIgnoreRule[];
  loading: boolean;
  submitting: boolean;
  deletingId: number | null;
  formatRule: (senderId: string | null | undefined, groupId: string | null | undefined) => string;
}>();

const emit = defineEmits<{
  edit: [rule: QqChatAgentServiceIgnoreRule];
  remove: [ruleId: number];
}>();
</script>

<style scoped lang="scss">
@use "./ignore-rules-list" as *;
</style>
