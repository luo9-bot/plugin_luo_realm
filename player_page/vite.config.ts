import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// base 用相对路径：同一份 dist 既可挂在插件 /player/ 下，也可部署到
// Cloudflare Pages 子目录。API 地址通过 VITE_API_BASE 注入，默认同源。
export default defineConfig({
  base: "./",
  plugins: [vue()],
  server: {
    proxy: {
      "/api": "http://127.0.0.1:18780",
    },
  },
});
