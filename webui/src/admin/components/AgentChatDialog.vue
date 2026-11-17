<template>
    <t-dialog :visible="!!agent" :header="agent?.name || '对话'" width="90vw" :footer="false" @close="$emit('close')">
        <template #header><t-space>{{ agent?.name }}<t-button variant="text"
                    @click="openWindow">在新窗口打开</t-button></t-space></template>
        <div class="agent-chat-dialog">
            <Chat v-if="agent" :key="agent.config_id" :agent-id="agent.config_id" :session-id="sessionId" embedded
                @update:session-id="sessionId = $event" />
        </div>
    </t-dialog>
</template>
<script setup lang="ts">
import { ref, watch } from "vue";
import { useRouter } from "vue-router";
import Chat from "../chat/Chat.vue";
import type { ServiceWithRuntime } from "../../api/client";
const props = defineProps<{ agent: ServiceWithRuntime | null | undefined }>();
defineEmits<{ (e: "close"): void }>();
const router = useRouter();
const sessionId = ref("");
watch(() => props.agent?.config_id, () => { sessionId.value = ""; });
function openWindow() {
    if (!props.agent) return;
    window.open(router.resolve({ path: "/chat", query: { agent_id: props.agent.config_id, ...(sessionId.value ? { session_id: sessionId.value } : {}) } }).href, "_blank");
}
</script>
<style scoped>
.agent-chat-dialog {
    height: 76vh;
    min-height: 350px;
}
</style>
