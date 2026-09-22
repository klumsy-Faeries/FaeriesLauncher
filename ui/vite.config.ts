import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Themes and locales live at the repo root so non-developers can find
    // them; allow the dev server to serve them from outside ui/.
    fs: { allow: [".."] },
  },
  resolve: {
    alias: {
      "@themes": fileURLToPath(new URL("../themes", import.meta.url)),
      "@locales": fileURLToPath(new URL("../locales", import.meta.url)),
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "esnext",
    outDir: "dist",
  },
});
