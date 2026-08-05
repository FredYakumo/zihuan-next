<template>
  <section class="page task-execution-page">
    <AdminPageHeader title="任务执行过程" />
    <t-card bordered>
      <template #actions>
        <t-button variant="outline" @click="followLatest = !followLatest">{{ followLatest ? "停止跟随" : "跟随实时过程" }}</t-button>
        <t-button variant="outline" @click="openLogs">详细日志</t-button>
        <t-button variant="text" @click="router.push('/tasks')">返回任务列表</t-button>
      </template>
      <div class="execution-meta">任务 {{ taskId }} · {{ events.length }} 个执行事件</div>
      <div ref="graphEl" class="execution-graph">
        <div v-if="events.length === 0" class="empty-state">等待 Agent 开始执行…</div>
        <template v-for="event in events" :key="event.seq">
          <div class="trace-node" :class="[`trace-node--${event.status}`, { 'trace-node--running': event.status === 'running' }]" @click="selected = event">
            <span class="trace-node__type">{{ eventLabel(event) }}</span>
            <strong>{{ eventTitle(event) }}</strong>
            <small>#{{ event.seq }} · {{ formatTime(event.timestamp) }}</small>
          </div>
          <div v-if="event !== events[events.length - 1]" class="trace-link"><span /></div>
        </template>
      </div>
    </t-card>

    <t-dialog :visible="!!selected" header="执行节点详情" width="760px" @close="selected = null">
      <pre class="trace-payload">{{ selected ? JSON.stringify(selected.payload, null, 2) : '' }}</pre>
    </t-dialog>
    <t-dialog :visible="showLogs" header="详细日志" width="900px" @close="showLogs = false">
      <div class="trace-logs"><div v-for="(entry, index) in logs" :key="index"><span>{{ entry.timestamp }}</span> {{ entry.level }} {{ entry.message }}</div></div>
    </t-dialog>
  </section>
</template>

<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { tasks, type TaskLogEntry, type TaskTraceEvent } from "../../api/client";
import { ws } from "../../api/ws";

const route = useRoute();
const router = useRouter();
const taskId = String(route.params.taskId ?? "");
const events = ref<TaskTraceEvent[]>([]);
const selected = ref<TaskTraceEvent | null>(null);
const showLogs = ref(false);
const logs = ref<TaskLogEntry[]>([]);
const followLatest = ref(true);
const graphEl = ref<HTMLElement | null>(null);
let unsubscribe: (() => void) | null = null;

function eventLabel(event: TaskTraceEvent): string {
  return ({ user_message: "用户消息", llm_request: "模型请求", llm_response: "模型响应", tool_calls_requested: "工具规划", tool: "工具调用", reply_send: "发送回复" } as Record<string, string>)[event.event_type] ?? event.event_type;
}
function eventTitle(event: TaskTraceEvent): string {
  const payload = event.payload as Record<string, unknown>;
  return String(payload.name ?? payload.call_id ?? payload.assistant_content ?? eventLabel(event));
}
function formatTime(value: string): string { return new Date(value).toLocaleTimeString(); }
async function loadTrace(): Promise<void> {
  const result = await tasks.trace(taskId);
  events.value = result.events.sort((a, b) => a.seq - b.seq);
  await nextTick();
  scrollLatest();
}
function append(event: TaskTraceEvent): void {
  if (events.value.some((item) => item.seq === event.seq)) return;
  events.value.push(event);
  events.value.sort((a, b) => a.seq - b.seq);
  void nextTick().then(scrollLatest);
}
function scrollLatest(): void { if (followLatest.value && graphEl.value) graphEl.value.scrollTop = graphEl.value.scrollHeight; }
async function openLogs(): Promise<void> { logs.value = (await tasks.logs(taskId, { limit: 500, offset: 0 })).entries; showLogs.value = true; }
onMounted(() => {
  void loadTrace();
  unsubscribe = ws.onMessage((message) => { if (message.type === "TaskTraceEvent" && message.event.task_id === taskId) append(message.event); });
});
onUnmounted(() => unsubscribe?.());
</script>

<style scoped lang="scss">
.execution-meta { color: var(--td-text-color-secondary); margin-bottom: 12px; }
.execution-graph { min-height: 420px; max-height: 68vh; overflow: auto; padding: 28px; background: radial-gradient(circle at 1px 1px, rgba(130,140,160,.22) 1px, transparent 0) 0 0/18px 18px; }
.trace-node { width: min(620px, 96%); margin: auto; padding: 14px 18px; border: 1px solid var(--td-component-border); border-left: 5px solid #5b8ff9; border-radius: 8px; background: var(--td-bg-color-container); cursor: pointer; box-shadow: 0 3px 10px rgba(0,0,0,.07); }
.trace-node strong, .trace-node small, .trace-node__type { display: block; }.trace-node small { color: var(--td-text-color-secondary); margin-top: 5px; }.trace-node__type { color: #5b8ff9; font-size: 12px; margin-bottom: 4px; }
.trace-node--failed { border-left-color: #e34d59; }.trace-node--failed .trace-node__type { color: #e34d59; }.trace-node--running { border-left-color: #ed7b2f; animation: pulse 1.4s infinite; }
.trace-link { height: 36px; display: grid; place-items: center; }.trace-link span { width: 2px; height: 100%; background: #5b8ff9; position: relative; }.trace-link span::after { content: ""; position: absolute; bottom: -2px; left: -4px; border: 5px solid transparent; border-top-color: #5b8ff9; }
.trace-payload, .trace-logs { max-height: 60vh; overflow: auto; white-space: pre-wrap; word-break: break-word; background: var(--td-bg-color-secondarycontainer); padding: 12px; }.trace-logs div { padding: 4px 0; border-bottom: 1px solid var(--td-component-stroke); }.trace-logs span { color: var(--td-text-color-secondary); }
@keyframes pulse { 50% { box-shadow: 0 0 0 7px rgba(237,123,47,.18); } }
</style>
