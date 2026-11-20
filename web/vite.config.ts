import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";

// The version comes from the Rust workspace, so the panel always shows the
// same version as the binary it is built into.
function workspaceVersion(): string {
  try {
    const cargo = readFileSync(new URL("../Cargo.toml", import.meta.url), "utf8");
    const v = /\[workspace\.package\][^[]*?version\s*=\s*"([^"]+)"/.exec(cargo)?.[1];
    if (v) return v;
  } catch {
    /* building the panel on its own */
  }
  return JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")).version;
}
const version = workspaceVersion();

// During development the panel is served by Vite and API calls are proxied
// to a running xbncd (web address 127.0.0.1:8080 by default).
export default defineConfig({
  plugins: [react()],
  define: { __XBNC_VERSION__: JSON.stringify(version) },
  server: {
    port: 5173,
    proxy: {
      "/api": { target: "http://127.0.0.1:8080", ws: true, changeOrigin: false },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    chunkSizeWarningLimit: 900,
  },
});
