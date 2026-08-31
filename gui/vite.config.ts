import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  // Vite options tailored for Tauri: don't hide Rust errors, use a fixed port.
  // 5173/5174 rather than Tauri's default 1420/1421: on Windows those fall in a
  // Hyper-V reserved range (netsh interface ipv4 show excludedportrange tcp),
  // where binding fails with EACCES even though nothing is listening.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 5174 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
