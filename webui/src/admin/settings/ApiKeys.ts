import { ref } from "vue";

import { useSettings } from "./useSettings";

export function useApiKeysPage() {
  const {
    modelHttpApiKeys,
    newModelHttpSecret,
    createModelHttpApiKey,
    updateModelHttpApiKey,
    deleteModelHttpApiKey,
    copyModelHttpSecret,
  } = useSettings();

  const secretDialogVisible = ref(false);
  const columns = [
    { colKey: "name", title: "名称" },
    { colKey: "secret_prefix", title: "Key 前缀" },
    { colKey: "created_at", title: "创建时间" },
    { colKey: "expires_at", title: "过期时间" },
    { colKey: "group", title: "分组" },
    { colKey: "enabled", title: "启用", width: 80 },
    { colKey: "actions", title: "操作", width: 80 },
  ];

  async function handleCreateModelHttpApiKey() {
    await createModelHttpApiKey();
    if (newModelHttpSecret.value) secretDialogVisible.value = true;
  }

  return {
    modelHttpApiKeys,
    newModelHttpSecret,
    updateModelHttpApiKey,
    deleteModelHttpApiKey,
    copyModelHttpSecret,
    secretDialogVisible,
    columns,
    handleCreateModelHttpApiKey,
  };
}

export type UseApiKeysPageReturn = ReturnType<typeof useApiKeysPage>;
