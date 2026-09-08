import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// @ts-expect-error process は nodejs グローバル
const host = process.env.TAURI_DEV_HOST;

// Tauri 開発向け設定 (Kataribe と同型)。port は Kataribe (1420) と衝突しないよう 1421。
export default defineConfig(async () => ({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1421,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1422 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
}));
