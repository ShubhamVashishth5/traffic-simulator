import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

/** `marg serve` endpoints, proxied in `npm run dev` (the viewer is same-origin in production). */
const server = process.env.MARG_SERVER ?? "http://127.0.0.1:8080";
const proxied = ["/version", "/register", "/scenarios", "/sessions", "/runs", "/files"];

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: Object.fromEntries(proxied.map((p) => [p, { target: server, ws: p === "/sessions" }])),
  },
  optimizeDeps: { exclude: ["@duckdb/duckdb-wasm"] },
  build: { chunkSizeWarningLimit: 4000 },
  test: { environment: "node" },
});
