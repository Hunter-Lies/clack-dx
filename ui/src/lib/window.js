import { getCurrentWindow } from "@tauri-apps/api/window";

// 在浏览器里单独调 UI 时没有 Tauri 宿主，这些调用要静默跳过。
export const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function minimizeWindow() {
  if (!inTauri) return;
  await getCurrentWindow().minimize();
}

/// 无边框窗口自己拖。挂在整个机身的空白处上，交互元素（屏幕/按键/链接）里不拖。
export async function startDrag() {
  if (!inTauri) return;
  await getCurrentWindow().startDragging();
}

export async function closeWindow() {
  if (!inTauri) return;
  await getCurrentWindow().close();
}
