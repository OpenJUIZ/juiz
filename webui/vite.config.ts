import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// 開発サーバー(npm run dev)は /api と /api-docs を juiz の HTTP Broker へ中継する。
// 中継先は環境変数 JUIZ_URL で変えられる(既定は http://localhost:8000)。
const juizUrl = process.env.JUIZ_URL ?? "http://localhost:8000";

export default defineConfig({
  plugins: [react()],
  // static_filepaths でどの URL に置いても動くよう、相対パスで出力する
  base: "./",
  server: {
    proxy: {
      "/api": { target: juizUrl, changeOrigin: true },
      "/api-docs": { target: juizUrl, changeOrigin: true },
    },
  },
});
