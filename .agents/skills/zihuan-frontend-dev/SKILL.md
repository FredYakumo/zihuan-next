---
name: zihuan-frontend-dev
description: Develop the ZiHuan Next WebUI. Use for Vue admin views, TypeScript API clients, composables, styles, the Litegraph editor, or any changes below webui/.
---

# WebUI Development

- `webui/` uses Vue 3, TypeScript, Vite, TDesign, and pnpm. Run frontend commands from that directory.
- The admin app and `/editor` graph editor have separate entry paths. Confirm the target before changing shared bootstrap code.
- Keep shared admin types in `webui/src/admin/model.ts`; put REST calls in `webui/src/api/` and reuse composables for shared view state.
- Match existing TDesign patterns and use `tdesign-icons-vue-next` for recognizable actions. Keep TypeScript strict and avoid `any`.
- Use `pnpm run build` to type-check and build. Use `pnpm run dev` for iterative UI work.
