import { ref } from "vue";

export interface ConfigImportDialogProps {
  visible: boolean;
  title: string;
  createLabel: string;
  loading: boolean;
}

export interface ConfigImportDialogEmit {
  (event: "update:visible", visible: boolean): void;
  (event: "create"): void;
  (event: "clipboard-import"): void;
  (event: "file-change", value: Event): void;
}

export function useConfigImportDialog(
  _props: ConfigImportDialogProps,
  _emit: ConfigImportDialogEmit,
) {
  const fileInput = ref<HTMLInputElement | null>(null);

  return { fileInput };
}
