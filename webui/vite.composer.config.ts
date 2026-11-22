import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [vue()],
  root: resolve(__dirname, "composer"),
  base: "/",
  build: {
    outDir: resolve(__dirname, "../zihuan_composer_gui/ui/dist"),
    emptyOutDir: true,
  },
});
