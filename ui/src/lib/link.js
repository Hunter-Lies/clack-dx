import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./window";

// 打开外部链接。
// Tauri 里交给后端用 ShellExecuteW 叫系统浏览器；不用 window.open——那在 WebView2 里
// 要额外配权限，配不齐就变成在窗口里再开一个 WebView。
// 浏览器预览（pnpm dev）时后端不在，退回 window.open。
export async function openExternal(url) {
  if (inTauri) {
    await invoke("open_url", { url });
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}
