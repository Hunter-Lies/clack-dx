import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// base 用相对路径，构建产物可以直接用 file:// 打开看效果
export default defineConfig({
  plugins: [vue()],
  base: "./",
  clearScreen: false,
  server: { port: 1430, strictPort: true, host: "127.0.0.1" },
});
