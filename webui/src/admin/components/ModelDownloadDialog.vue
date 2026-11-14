<template>
  <t-dialog
    :visible="visible"
    header="下载模型权重"
    width="560px"
    :close-on-overlay-click="!jobActive"
    :footer="false"
    @close="handleClose"
  >
    <div class="model-download-dialog">
      <p class="model-download-site">下载站点：<code>{{ endpoint }}</code></p>

      <div class="model-download-presets">
        <label v-for="preset in presets" :key="presetKey(preset)" class="model-download-preset">
          <input v-model="selectedPreset" type="radio" :value="presetKey(preset)" :disabled="jobActive" />
          <span class="model-download-preset-name">
            {{ preset.repo_id }}<template v-if="preset.file"> · {{ preset.file }}</template>
          </span>
          <span class="model-download-preset-size">{{ preset.size }}</span>
        </label>
      </div>

      <t-input
        v-model="customRepo"
        clearable
        :disabled="jobActive"
        placeholder="自定义 repo id(Owner/Model)"
      />
      <t-input
        v-if="isLlm"
        v-model="customFile"
        clearable
        :disabled="jobActive"
        placeholder="GGUF 权重文件名（可选，留空自动选取仓库内唯一的 .gguf）"
      />

      <div v-if="job" class="model-download-progress">
        <div class="model-download-progress-head">
          <strong>{{ job.repo_id }}</strong>
          <t-space size="small">
            <t-button
              v-if="jobActive"
              variant="text"
              theme="danger"
              size="small"
              @click="cancelJob"
            >
              终止
            </t-button>
            <t-tag v-if="job.status === 'completed'" variant="light" theme="success">下载完成</t-tag>
            <t-tag v-else-if="job.status === 'failed'" variant="light" theme="danger">下载失败</t-tag>
            <t-tag v-else-if="job.status === 'cancelled'" variant="light">已终止</t-tag>
            <t-tag v-else variant="light">{{ job.status === "queued" ? "排队中" : "下载中" }}</t-tag>
          </t-space>
        </div>
        <t-progress
          v-if="job.status === 'downloading' || job.status === 'queued'"
          :percentage="progressPercent"
          :status="job.status === 'queued' ? 'warning' : 'active'"
        />
        <p v-if="job.current_file" class="model-download-current">正在下载：{{ job.current_file }}</p>
        <p v-if="job.error" class="model-download-error">{{ job.error }}</p>
      </div>

      <p v-if="error" class="model-download-error" role="alert">{{ error }}</p>

      <div class="model-download-footer">
        <t-button variant="outline" @click="handleClose">{{ jobActive ? "后台运行" : "取消" }}</t-button>
        <t-button theme="primary" :disabled="jobActive || !repoId" :loading="starting" @click="startDownload">
          开始下载
        </t-button>
      </div>
    </div>
  </t-dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";

import {
  getModelDownloadSettings,
  modelWeights,
  type ModelWeightDownloadJob,
} from "../../api/client";

interface PresetModel {
  repo_id: string;
  size: string;
  file?: string;
  tokenizer_repo?: string;
}

const props = withDefaults(defineProps<{ visible: boolean; kind?: string }>(), {
  kind: "text_embedding",
});
const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "completed", modelName: string): void;
}>();

const EMBEDDING_PRESETS: PresetModel[] = [
  { repo_id: "Qwen/Qwen3-Embedding-0.6B", size: "推荐 · 约 1.2 GB" },
  { repo_id: "Qwen/Qwen3-Embedding-4B", size: "约 4 GB" },
  { repo_id: "Qwen/Qwen3-Embedding-8B", size: "约 16 GB" },
];

const LLM_PRESETS: PresetModel[] = [
  {
    repo_id: "Qwen/Qwen3-0.6B-GGUF",
    file: "Qwen3-0.6B-Q8_0.gguf",
    tokenizer_repo: "Qwen/Qwen3-0.6B",
    size: "约 0.65 GB",
  },
  {
    repo_id: "Qwen/Qwen3-1.7B-GGUF",
    file: "Qwen3-1.7B-Q8_0.gguf",
    tokenizer_repo: "Qwen/Qwen3-1.7B",
    size: "约 1.8 GB",
  },
  {
    repo_id: "Qwen/Qwen3-4B-GGUF",
    file: "Qwen3-4B-Q4_K_M.gguf",
    tokenizer_repo: "Qwen/Qwen3-4B",
    size: "约 2.5 GB",
  },
  {
    repo_id: "Qwen/Qwen3-8B-GGUF",
    file: "Qwen3-8B-Q4_K_M.gguf",
    tokenizer_repo: "Qwen/Qwen3-8B",
    size: "约 5 GB",
  },
];

const isLlm = computed(() => props.kind === "llm");
const presets = computed(() => (isLlm.value ? LLM_PRESETS : EMBEDDING_PRESETS));

const selectedPreset = ref("");
const customRepo = ref("");
const customFile = ref("");
const endpoint = ref("https://huggingface.co");
const starting = ref(false);
const error = ref<string | null>(null);
const job = ref<ModelWeightDownloadJob | null>(null);
let pollTimer: number | null = null;

function presetKey(preset: PresetModel): string {
  return preset.file ? `${preset.repo_id}#${preset.file}` : preset.repo_id;
}

const selectedPresetModel = computed(
  () => presets.value.find((preset) => presetKey(preset) === selectedPreset.value) ?? null,
);

const repoId = computed(() => {
  const custom = customRepo.value.trim();
  return custom || selectedPresetModel.value?.repo_id || "";
});

const jobActive = computed(
  () => job.value?.status === "queued" || job.value?.status === "downloading",
);

const progressPercent = computed(() => {
  const current = job.value;
  if (!current || current.total_bytes <= 0) return 0;
  return Math.min(100, Math.round((current.downloaded_bytes / current.total_bytes) * 100));
});

async function loadEndpoint() {
  try {
    const data = await getModelDownloadSettings();
    endpoint.value = data.endpoint;
  } catch {
    // keep the default display
  }
}

async function pollJob(jobId: string) {
  try {
    const data = await modelWeights.listDownloadJobs();
    const found = data.jobs.find((item) => item.id === jobId) ?? null;
    const wasActive = jobActive.value;
    job.value = found;
    if (found?.status === "completed" && wasActive) {
      stopPolling();
      emit("completed", found.dest_dir.split(/[\\/]/).pop() ?? found.repo_id.split("/").pop() ?? "");
    } else if (found?.status === "failed" || found?.status === "cancelled") {
      stopPolling();
    }
  } catch {
    // transient polling errors are ignored; the next tick retries
  }
}

function startPolling(jobId: string) {
  stopPolling();
  pollTimer = window.setInterval(() => pollJob(jobId), 1500);
}

function stopPolling() {
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
}

async function startDownload() {
  error.value = null;
  starting.value = true;
  try {
    const isCustom = Boolean(customRepo.value.trim());
    const preset = isCustom ? null : selectedPresetModel.value;
    const opts =
      !isLlm.value
        ? undefined
        : isCustom
          ? { file: customFile.value.trim() || undefined }
          : { file: preset?.file, tokenizer_repo: preset?.tokenizer_repo };
    await modelWeights.download(repoId.value, props.kind, opts);
    // Fetch the freshest job (the one just created) and track it.
    const data = await modelWeights.listDownloadJobs();
    const latest = data.jobs.find(
      (item) => item.repo_id === repoId.value && (item.status === "queued" || item.status === "downloading"),
    );
    if (latest) {
      job.value = latest;
      startPolling(latest.id);
    }
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    starting.value = false;
  }
}

async function cancelJob() {
  if (!job.value) return;
  try {
    await modelWeights.cancelDownloadJob(job.value.id);
  } catch (e) {
    error.value = (e as Error).message;
  }
}

function handleClose() {
  if (jobActive.value) {
    // Keep downloading in the background; just dismiss the dialog.
    stopPolling();
    job.value = null;
  }
  emit("update:visible", false);
}

watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      error.value = null;
      customRepo.value = "";
      customFile.value = "";
      selectedPreset.value = presets.value.length ? presetKey(presets.value[0]) : "";
      loadEndpoint();
    } else if (!jobActive.value) {
      stopPolling();
      job.value = null;
    }
  },
  { immediate: true },
);
</script>

<style scoped lang="scss">
.model-download-dialog {
  display: grid;
  gap: 12px;
}

.model-download-site {
  margin: 0;
  font-size: 12px;
  opacity: 0.8;
}

.model-download-presets {
  display: grid;
  gap: 6px;
}

.model-download-preset {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border: 1px solid var(--admin-border, rgba(255, 255, 255, 0.12));
  border-radius: 6px;
  cursor: pointer;

  input {
    accent-color: var(--td-brand-color);
  }
}

.model-download-preset-name {
  flex: 1;
  font-size: 13px;
}

.model-download-preset-size {
  font-size: 12px;
  opacity: 0.65;
}

.model-download-hint {
  margin: 0;
  font-size: 12px;
  opacity: 0.65;
}

.model-download-progress {
  display: grid;
  gap: 6px;
  padding: 10px 12px;
  border: 1px solid var(--admin-border, rgba(255, 255, 255, 0.12));
  border-radius: 6px;
}

.model-download-progress-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 13px;
}

.model-download-current {
  margin: 0;
  font-size: 12px;
  opacity: 0.7;
}

.model-download-error {
  margin: 0;
  font-size: 12px;
  color: var(--td-error-color);
}

.model-download-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
