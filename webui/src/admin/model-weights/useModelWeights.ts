import { computed, onMounted, onBeforeUnmount, ref } from "vue";

import {
  modelWeights,
  type ModelWeightDownloadJob,
  type ModelWeightGroup,
} from "../../api/client";

export function useModelWeights() {
  const groups = ref<ModelWeightGroup[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const importLoading = ref<string | null>(null);

  const downloadDialogVisible = ref(false);
  const downloadKind = ref("text_embedding");

  function openDownload(kind: string) {
    downloadKind.value = kind;
    downloadDialogVisible.value = true;
  }

  const jobs = ref<ModelWeightDownloadJob[]>([]);
  const activeJobs = computed(() =>
    jobs.value.filter((job) => job.status === "queued" || job.status === "downloading"),
  );

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      const data = await modelWeights.list();
      groups.value = data.groups;
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  async function reloadJobs() {
    try {
      const data = await modelWeights.listDownloadJobs();
      const previous = new Map(jobs.value.map((job) => [job.id, job.status]));
      jobs.value = data.jobs;
      // Refresh the listing when a job settles so new weights show up immediately.
      const settled = data.jobs.some((job) => {
        const before = previous.get(job.id);
        return before && before !== job.status && job.status !== "queued" && job.status !== "downloading";
      });
      if (settled) await load();
    } catch {
      // polling errors are transient
    }
  }

  let jobsTimer: number | null = null;

  onMounted(() => {
    load();
    jobsTimer = window.setInterval(reloadJobs, 1500);
  });

  onBeforeUnmount(() => {
    if (jobsTimer !== null) window.clearInterval(jobsTimer);
  });

  const importError = ref<string | null>(null);

  async function importWeights(kind: string) {
    if (importLoading.value) return;
    importLoading.value = kind;
    importError.value = null;
    try {
      const data = await modelWeights.import(kind);
      if (!data.cancelled) await load();
    } catch (e) {
      importError.value = (e as Error).message;
    } finally {
      importLoading.value = null;
    }
  }

  const renameDialogVisible = ref(false);
  const renameTarget = ref<{ kind: string; name: string } | null>(null);
  const renameName = ref("");
  const renameSaving = ref(false);
  const renameError = ref<string | null>(null);

  function openRename(kind: string, name: string) {
    renameTarget.value = { kind, name };
    renameName.value = name;
    renameError.value = null;
    renameDialogVisible.value = true;
  }

  function closeRename() {
    renameDialogVisible.value = false;
    renameTarget.value = null;
  }

  async function confirmRename() {
    const target = renameTarget.value;
    if (!target) return;
    const newName = renameName.value.trim();
    if (!newName || newName === target.name) {
      closeRename();
      return;
    }
    renameSaving.value = true;
    renameError.value = null;
    try {
      await modelWeights.rename(target.kind, target.name, newName);
      closeRename();
      await load();
    } catch (e) {
      renameError.value = (e as Error).message;
    } finally {
      renameSaving.value = false;
    }
  }

  const removing = ref(new Set<string>());

  async function removeWeights(kind: string, name: string) {
    const key = `${kind}/${name}`;
    if (removing.value.has(key)) return;
    removing.value.add(key);
    try {
      await modelWeights.remove(kind, name);
      await load();
    } catch (e) {
      error.value = (e as Error).message;
    } finally {
      removing.value.delete(key);
    }
  }

  async function openLocalDir(kind: string) {
    try {
      await modelWeights.openDir(kind);
    } catch (e) {
      error.value = (e as Error).message;
    }
  }

  async function cancelJob(id: string) {
    try {
      await modelWeights.cancelDownloadJob(id);
      await reloadJobs();
    } catch (e) {
      error.value = (e as Error).message;
    }
  }

  function onDownloadCompleted() {
    void load();
  }

  function formatBytes(bytes: number): string {
    if (!bytes) return "—";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
    return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
  }

  const columns = [
    { colKey: "name", title: "名称", width: 260 },
    { colKey: "status", title: "状态", width: 240 },
    { colKey: "size", title: "大小", width: 120 },
    { colKey: "actions", title: "操作", width: 160, fixed: "right" as const },
  ];

  return {
    groups,
    loading,
    error,
    columns,
    formatBytes,
    load,
    importLoading,
    importError,
    importWeights,
    openLocalDir,
    downloadDialogVisible,
    downloadKind,
    openDownload,
    jobs,
    activeJobs,
    cancelJob,
    onDownloadCompleted,
    renameDialogVisible,
    renameTarget,
    renameName,
    renameSaving,
    renameError,
    openRename,
    closeRename,
    confirmRename,
    removeWeights,
    removing,
  };
}

export type UseModelWeightsReturn = ReturnType<typeof useModelWeights>;
