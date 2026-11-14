import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { setup as setupApi, system as systemApi, type BuildInfo } from "../../api/client";
import { onThemeChange } from "../../ui/theme";

const SIDEBAR_COLLAPSED_KEY = "zihuan-admin-sidebar-collapsed";
const MOBILE_QUERY = "(max-width: 900px)";

export function useAdminApp() {
  const route = useRoute();
  const router = useRouter();
  const isSetupRoute = computed(() => route.path === "/setup");
  const sidebarOpen = ref(false);
  const sidebarCollapsed = ref(localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === "1");

  const showOverlay = computed(() => {
    return sidebarOpen.value && window.matchMedia(MOBILE_QUERY).matches;
  });

  watch(sidebarCollapsed, (collapsed) => {
    localStorage.setItem(SIDEBAR_COLLAPSED_KEY, collapsed ? "1" : "0");
  });

  function closeSidebar() {
    sidebarOpen.value = false;
  }

  function toggleSidebar() {
    if (window.matchMedia(MOBILE_QUERY).matches) {
      sidebarOpen.value = !sidebarOpen.value;
    } else {
      sidebarCollapsed.value = !sidebarCollapsed.value;
    }
  }

  const activeMenuValue = computed(() => route.path.startsWith("/data-explorer/") ? "/data-explorer" : route.path);

  function readMenuTheme(): "light" | "dark" {
    return document.documentElement.getAttribute("theme-mode") === "light" ? "light" : "dark";
  }

  const menuTheme = ref(readMenuTheme());
  let unsubscribeTheme: (() => void) | undefined;
  onMounted(() => { unsubscribeTheme = onThemeChange(() => { menuTheme.value = readMenuTheme(); }); });
  onUnmounted(() => { unsubscribeTheme?.(); });

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

  const buildInfo = ref<BuildInfo | null>(null);

  onMounted(async () => {
    try {
      buildInfo.value = await systemApi.getBuildInfo();
    } catch {
      // The build label is informational only; an older backend without the
      // endpoint must not break the shell.
    }
  });

  function openSetupWizard() {
    closeSidebar();
    router.push("/setup");
  }

  return {
    isSetupRoute,
    sidebarOpen,
    sidebarCollapsed,
    showOverlay,
    closeSidebar,
    toggleSidebar,
    activeMenuValue,
    menuTheme,
    buildInfo,
    openSetupWizard,
  };
}

export type UseAdminAppReturn = ReturnType<typeof useAdminApp>;
