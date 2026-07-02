import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { setup as setupApi } from "../../api/client";

export function useAdminApp() {
  const route = useRoute();
  const router = useRouter();
  const isSetupRoute = computed(() => route.path === "/setup");
  const sidebarOpen = ref(false);

  const showOverlay = computed(() => {
    return sidebarOpen.value && window.matchMedia("(max-width: 900px)").matches;
  });

  function closeSidebar() {
    sidebarOpen.value = false;
  }

  onMounted(async () => {
    try {
      const status = await setupApi.getStatus();
      if (!status.completed && !status.skipped && router.currentRoute.value.path !== "/setup") {
        router.push("/setup");
      }
    } catch {
      // fail open
    }
  });

  return {
    isSetupRoute,
    sidebarOpen,
    showOverlay,
    closeSidebar,
  };
}

export type UseAdminAppReturn = ReturnType<typeof useAdminApp>;
