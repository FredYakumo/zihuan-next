import { ref } from "vue";

export interface ConnectionCredentialInputProps {
  placeholder: string;
  inputClass: string;
}

export function useConnectionCredentialInput(_props: ConnectionCredentialInputProps) {
  const isVisible = ref(false);

  return { isVisible };
}
