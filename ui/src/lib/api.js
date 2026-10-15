import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { inTauri } from "./window";

// 后端命令都在 src-tauri/src/lib.rs 里；浏览器里单独调 UI 时全部空转。

export function scan() {
  return inTauri ? invoke("scan") : Promise.resolve([]);
}

/// rows = 界面上那几行产品的 id（后端自己展开成 x86 / x64 两个安装器）
export function installComponents(rows) {
  return inTauri ? invoke("install", { rows }) : Promise.resolve();
}

export function isElevated() {
  return inTauri ? invoke("is_elevated") : Promise.resolve(true);
}

/// 返回 true = 已经用管理员身份重新拉起自己，当前进程该退场了
export function requestElevation(interactive = false) {
  return inTauri ? invoke("request_elevation", { interactive }) : Promise.resolve(false);
}

/// 关不掉窗口时的兜底：直接结束进程
export function quitApp() {
  return inTauri ? invoke("quit") : Promise.resolve();
}

export function onProgress(cb) {
  return inTauri
    ? listen("install:progress", (e) => cb(e.payload))
    : Promise.resolve(() => {});
}

export function onDone(cb) {
  return inTauri
    ? listen("install:done", (e) => cb(e.payload))
    : Promise.resolve(() => {});
}
