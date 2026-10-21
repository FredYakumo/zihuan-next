<template>
  <div class="context-sub-agent" :class="{ 'context-sub-agent--nested': node.depth > 1 }">
    <div class="context-sub-agent-head">
      <span class="context-sub-agent-badge">Sub Agent</span>
      <span class="context-sub-agent-name">{{ node.name }}</span>
      <span class="context-sub-agent-status">
        <t-loading v-if="!finished" size="12px" />
        <span v-else class="muted">已结束</span>
      </span>
    </div>

    <div class="context-sub-agent-body">
      <div v-if="node.input" class="context-sub-agent-section">
        <div class="context-sub-agent-label">输入</div>
        <pre class="context-sub-agent-pre">{{ node.input.user_prompt }}</pre>
        <template v-if="node.input.system_prompt">
          <button
            type="button"
            class="context-sub-agent-toggle"
            @click="systemPromptOpen = !systemPromptOpen"
          >
            <span class="context-thinking-icon">{{ systemPromptOpen ? "▾" : "▸" }}</span>
            系统提示词
            <span class="context-sub-agent-hint">{{ node.input.system_prompt.length }} 字</span>
          </button>
          <pre v-if="systemPromptOpen" class="context-sub-agent-pre">{{ node.input.system_prompt }}</pre>
        </template>
      </div>

      <template v-for="(step, index) in node.steps" :key="`step:${index}`">
        <div v-if="step.kind === 'assistant'" class="context-sub-agent-message">
          <div class="context-sub-agent-message-head">
            Assistant
            <span v-if="step.message.final_response" class="context-sub-agent-tag">最终输出</span>
            <span v-else-if="step.message.iteration !== undefined" class="context-sub-agent-tag">
              第 {{ step.message.iteration }} 轮
            </span>
          </div>
          <div
            v-if="step.message.reasoning"
            class="context-thinking"
            :class="{ collapsed: !reasoningOpen(index) }"
          >
            <button type="button" class="context-thinking-toggle" @click="toggleReasoning(index)">
              <span class="context-thinking-icon">{{ reasoningOpen(index) ? "▾" : "▸" }}</span>
              思考过程
            </button>
            <pre v-if="reasoningOpen(index)" class="context-thinking-content">{{ step.message.reasoning }}</pre>
          </div>
          <div v-if="step.message.content" class="context-sub-agent-text">{{ step.message.content }}</div>
        </div>

        <div v-else class="context-sub-agent-tool">
          <div class="context-sub-agent-tool-head">
            <span class="context-tool-title">工具调用 · {{ step.call.name }}</span>
            <span class="context-tool-status">
              <t-loading v-if="step.call.running" size="12px" />
              <span v-else-if="step.call.durationMs !== undefined" class="muted">{{ step.call.durationMs }} ms</span>
            </span>
          </div>
          <JsonView v-if="step.call.arguments !== undefined" :source="jsonText(step.call.arguments)" />
          <!-- A sub-agent this call started renders in place, so the box follows the call order. -->
          <ContextSubAgentBox
            v-for="child in childrenFor(step.call.callId)"
            :key="child.key"
            :node="child"
            :running="step.call.running"
          />
          <template v-if="step.call.result !== undefined">
            <div class="context-tool-result-label">结果</div>
            <JsonView :source="step.call.result" />
          </template>
        </div>
      </template>

      <!-- The response being written right now, shown while its tokens stream in. -->
      <div v-if="node.live" class="context-sub-agent-message context-sub-agent-message--live">
        <div class="context-sub-agent-message-head">
          Assistant
          <t-loading size="10px" />
        </div>
        <div v-if="node.live.reasoning" class="context-thinking">
          <div class="context-thinking-toggle">思考过程</div>
          <pre class="context-thinking-content">{{ node.live.reasoning }}</pre>
        </div>
        <div v-if="node.live.content" class="context-sub-agent-text">{{ node.live.content }}</div>
      </div>

      <!-- Nested agents whose start event is missing (e.g. an old archive) still get a box. -->
      <ContextSubAgentBox
        v-for="child in orphanChildren"
        :key="child.key"
        :node="child"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";

import type { SubAgentNode } from "./ChatContextTurn";
import { subAgentMessages } from "./ChatContextTurn";
import JsonView from "./JsonView.vue";

const props = defineProps<{
  node: SubAgentNode;
  /** Whether the call that started this agent is still running; absent in archived turns. */
  running?: boolean;
}>();

// The sub-agent's prompt is shown as it was sent: the user message is the call's real input,
// while the system prompt is usually long, so it starts folded and can be opened on demand.
const systemPromptOpen = ref(false);
const closedReasoning = ref(new Set<number>());

function reasoningOpen(index: number): boolean {
  return !closedReasoning.value.has(index);
}

function toggleReasoning(index: number) {
  const next = new Set(closedReasoning.value);
  if (next.has(index)) next.delete(index);
  else next.add(index);
  closedReasoning.value = next;
}

function childrenFor(callId: string): SubAgentNode[] {
  return props.node.children.filter((child) => child.callId === callId);
}

const orphanChildren = computed(() => {
  const callIds = new Set(props.node.toolCalls.map((call) => call.callId));
  return props.node.children.filter((child) => !callIds.has(child.callId));
});

// A sub-agent reports its final assistant message when its loop ends; while the enclosing call
// is still running there is nothing more to report, so the header shows progress until then.
const finished = computed(() => {
  if (props.running) return false;
  return subAgentMessages(props.node).some((message) => message.final_response);
});

function jsonText(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2) ?? "";
  } catch {
    return String(value ?? "");
  }
}
</script>

<style scoped lang="scss">
@use "./ContextSubAgentBox" as *;
</style>
