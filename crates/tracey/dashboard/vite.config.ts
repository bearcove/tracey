import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

export default defineConfig({
  plugins: [preact()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
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
