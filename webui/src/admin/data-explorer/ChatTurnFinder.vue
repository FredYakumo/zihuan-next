<template>
  <t-dialog v-model:visible="visibleModel" header="查找对话" width="720px" :footer="false">
    <div class="chat-turn-finder">
      <div class="finder-search">
        <t-input
          v-model="query"
          clearable
          placeholder="搜索会话"
          @enter="search(1)"
        />
        <t-button theme="primary" :loading="loading" @click="search(1)">搜索</t-button>
      </div>
      <t-table v-if="items.length" row-key="turn_id" :data="items" :columns="columns" :loading="loading" :hover="true">
        <template #user="{ row }"><span :title="row.user_text">{{ row.user_text || "（无文本输入）" }}</span></template>
        <template #reply="{ row }"><span class="finder-reply" :title="row.assistant_text || ''">{{ replyPreview(row.assistant_text) }}</span></template>
        <template #date="{ row }">{{ formatTime(row.created_at) }}</template>
      </t-table>
      <div v-else-if="!loading" class="empty-state">未找到匹配的对话。</div>
      <t-pagination v-if="total" class="finder-pagination" :total="total" :current="page" :page-size="pageSize" show-jumper @change="onPageChange" />
    </div>
  </t-dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { PrimaryTableCol } from "tdesign-vue-next";

import { explorer, type ServiceChatTurnListItem } from "../../api/client";
import { formatTime } from "../model";

const props = defineProps<{
  serviceId: string;
  senderId: string;
  visible: boolean;
}>();

const emit = defineEmits<{ "update:visible": [value: boolean] }>();

const visibleModel = computed({
  get: () => props.visible,
  set: (value: boolean) => emit("update:visible", value),
});

const query = ref("");
const loading = ref(false);
const page = ref(1);
const pageSize = 20;
const total = ref(0);
const items = ref<ServiceChatTurnListItem[]>([]);

const columns: PrimaryTableCol<ServiceChatTurnListItem>[] = [
  { colKey: "user", title: "用户输入", ellipsis: true },
  { colKey: "reply", title: "bot 回复", width: 180 },
  { colKey: "date", title: "日期", width: 180 },
];

/** The reply column shows only the first few characters; the full text rides on the title. */
function replyPreview(text?: string): string {
  if (!text) return "（未回复）";
  return text.length > 5 ? `${text.slice(0, 5)}…` : text;
}

async function search(targetPage: number) {
  if (!props.senderId || loading.value) return;
  loading.value = true;
  try {
    const response = await explorer.queryServiceChatTurns(
      props.serviceId,
      props.senderId,
      query.value.trim(),
      targetPage,
      pageSize,
    );
    items.value = response.items;
    total.value = response.total;
    page.value = response.page;
  } catch (error: unknown) {
    alert((error as Error).message);
  } finally {
    loading.value = false;
  }
}

function onPageChange(info: { current: number; pageSize: number }) {
  void search(info.current);
}

// Opening the finder lists all conversations of the selected user, newest first.
watch(
  () => props.visible,
  (visible) => {
    if (!visible) return;
    query.value = "";
    page.value = 1;
    total.value = 0;
    items.value = [];
    void search(1);
  },
);
</script>

<style scoped lang="scss">
@use "./ChatTurnFinder" as *;
</style>
