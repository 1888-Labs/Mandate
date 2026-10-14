import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// In development the API (and the live event stream) come from mandate-server.
const api = process.env.MANDATE_API ?? "http://127.0.0.1:8080"

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: { "/v1": { target: api, changeOrigin: true } },
  },
  build: { outDir: "dist", sourcemap: true },
})
