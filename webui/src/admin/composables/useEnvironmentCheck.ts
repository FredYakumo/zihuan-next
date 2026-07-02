import { ref, onMounted, computed } from "vue";
import { setup as setupApi, type EnvironmentInfo } from "../../api/client";

export function useEnvironmentCheck() {
  const loading = ref(true);
  const info = ref<EnvironmentInfo>({
    os: "",
    os_detail: "",
    docker_available: false,
    docker_compose_available: false,
    cuda_version: null,
    compiler_version: null,
    proxy: null,
    services: [],
  });

  const compilerLabel = computed(() => {
    const os = info.value.os.toLowerCase();
    if (os === "windows") return "MSVC/GCC/LLVM";
    if (os === "macos") return "Clang/LLVM";
    return "GCC/LLVM";
  });

  onMounted(async () => {
    try {
      info.value = await setupApi.getEnvironment();
    } catch (err) {
      console.error("Failed to detect environment", err);
    } finally {
      loading.value = false;
    }
  });

  return {
    loading,
    info,
    compilerLabel,
  };
}

export type UseEnvironmentCheckReturn = ReturnType<typeof useEnvironmentCheck>;
