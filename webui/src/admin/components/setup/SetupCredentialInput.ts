import { onBeforeUnmount, onMounted, ref, type Ref } from "vue";

export interface SetupCredentialInputProps {
  inputType: "password" | "text";
}

export function useSetupCredentialInput(
  _props: SetupCredentialInputProps,
  value: Ref<string | null>,
) {
  const container = ref<HTMLElement | null>(null);
  const isOpen = ref(false);
  const isVisible = ref(false);

  const credentialCharacters = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

  function suggestCredential() {
    const randomBytes = new Uint32Array(32);
    crypto.getRandomValues(randomBytes);
    value.value = Array.from(randomBytes, (byte) => credentialCharacters[byte % credentialCharacters.length]).join("");
    isOpen.value = false;
  }

  function closeWhenClickingOutside(event: MouseEvent) {
    if (!container.value?.contains(event.target as Node)) {
      isOpen.value = false;
    }
  }

  onMounted(() => document.addEventListener("click", closeWhenClickingOutside));
  onBeforeUnmount(() => document.removeEventListener("click", closeWhenClickingOutside));

  return { container, isOpen, isVisible, suggestCredential };
}
