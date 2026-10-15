import { createApp } from "vue";
import App from "./App.vue";
import "./lib/theme";
import "./styles/console.css";

// 这是个桌面程序，不是网页：右键菜单、F5、Ctrl+R 都不该有反应。
// 一律生效（dev 也一样）——开发靠 Vite 的 HMR 就够，不用 F5，
// 这样开发时的表现和打包后完全一致，省得"dev 里好着、打包后失效"。
// F12 / Ctrl+Shift+I 留着，dev 里还要开 devtools。
document.addEventListener("contextmenu", (e) => e.preventDefault());
window.addEventListener("keydown", (e) => {
  const reload =
    e.key === "F5" || ((e.ctrlKey || e.metaKey) && (e.key === "r" || e.key === "R"));
  if (reload) e.preventDefault();
});

createApp(App).mount("#app");
