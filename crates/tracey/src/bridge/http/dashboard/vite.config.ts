import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

const dashboardRoot = fs.realpathSync.native(
  path.dirname(fileURLToPath(import.meta.url)),
);

export default defineConfig({
  root: dashboardRoot,
  plugins: [preact()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      input: path.join(dashboardRoot, "index.html"),
      output: {
        entryFileNames: "assets/index.js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
  server: {
    strictPort: false,
    port: 3030,
    host: "127.0.0.1",
    hmr: {
      // Client connects to tracey server, which proxies to Vite
      clientPort: 3000,
    },
  },
});
