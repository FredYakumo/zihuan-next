import { onMounted, onUnmounted, ref } from "vue";

export interface UseAdminClipboardOptions<T> {
  validate: (value: unknown) => T;
  onImport: (config: T) => void;
  isEnabled?: () => boolean;
}

export function useAdminClipboard<T>(options: UseAdminClipboardOptions<T>) {
  const copiedId = ref<string | null>(null);

  async function copyConfig(config: object, id: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(JSON.stringify(config, null, 2));
      copiedId.value = id;
      window.setTimeout(() => {
        if (copiedId.value === id) {
          copiedId.value = null;
        }
      }, 1600);
    } catch (error) {
      alert(`复制失败：${error instanceof Error ? error.message : String(error)}`);
    }
  }

  function importFromText(raw: string): void {
    try {
      const parsed = JSON.parse(raw);
      const config = options.validate(parsed);
      options.onImport(config);
    } catch (error) {
      alert(error instanceof Error ? error.message : String(error));
    }
  }

  function handlePaste(event: ClipboardEvent): void {
    if (options.isEnabled && !options.isEnabled()) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (
      target &&
      (target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable)
    ) {
      return;
    }
    const text = event.clipboardData?.getData("text");
    if (!text || !text.trim()) {
      return;
    }
    event.preventDefault();
    importFromText(text);
  }

  function handleFileChange(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) {
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      importFromText(String(reader.result));
      input.value = "";
    };
    reader.onerror = () => {
      alert("文件读取失败");
      input.value = "";
    };
    reader.readAsText(file);
  }

  onMounted(() => {
    document.addEventListener("paste", handlePaste);
  });

  onUnmounted(() => {
    document.removeEventListener("paste", handlePaste);
  });

  return {
    copiedId,
    copyConfig,
    handleFileChange,
  };
}
