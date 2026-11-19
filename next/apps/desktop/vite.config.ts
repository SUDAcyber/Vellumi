import { resolve } from "node:path";

import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

// The shell UI. Tauri loads the built files from ../dist (frontendDist);
// everything is bundled offline — the windows never touch the network.
export default defineConfig({
  root: resolve(import.meta.dirname, "ui"),
  base: "./",
  plugins: [svelte()],
  clearScreen: false,
  resolve: {
    // The editor package keeps its own node_modules, installed from the
    // lockfile the Swift app shipped (npm run install:editor): Markdown
    // round-tripping depends on those exact remark/micromark versions.
    alias: { "@vellumi/editor": resolve(import.meta.dirname, "../../packages/editor/src/main.js") },
  },
  build: {
    outDir: resolve(import.meta.dirname, "dist"),
    emptyOutDir: true,
    target: "safari16",
    chunkSizeWarningLimit: 8000,
    rollupOptions: {
      input: {
        index: resolve(import.meta.dirname, "ui/index.html"),
        settings: resolve(import.meta.dirname, "ui/settings.html"),
        projects: resolve(import.meta.dirname, "ui/projects.html"),
        cards: resolve(import.meta.dirname, "ui/cards.html"),
      },
    },
  },
});
