import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

export default defineConfig({
  plugins: [preact()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        // Predictable names for embedding via include_str!
        entryFileNames: "assets/index.js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
  server: {
    // Allow the tracey server to proxy to us
    strictPort: false,

    // Which port to listen on
    port: 3030,

    // Which address to listen on
    host: "127.0.0.1",

    // HMR will be proxied through the tracey server
    hmr: {
      // The tracey server proxies /__vite_hmr to us
      clientPort: 3000,
    },
  },
});
