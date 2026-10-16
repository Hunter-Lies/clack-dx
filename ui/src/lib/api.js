import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { inTauri } from "./window";

// 后端命令都在 src-tauri/src/lib.rs 里；浏览器里单独调 UI（pnpm dev）时本地演一遍。

/// 分组表，和 lib.rs 里的 GROUPS 一一对应。
/// 检测前那屏骨架要用它（还没跑检测，后端还没给数据），所以这里留一份。
export const GROUPS = [
  { id: "dx9", name: "DirectX 9 运行库" },
  { id: "dx10", name: "DirectX 10 运行库" },
  { id: "dx11", name: "DirectX 11 运行库" },
  { id: "shader", name: "着色器编译器 D3DCompiler" },
  { id: "audio", name: "XAudio / XACT 音频" },
  { id: "input", name: "XInput 手柄支持" },
];

// ---- 下面是浏览器预览用的假数据：真身这些全来自 payload 目录 ----

const DEMO_FILES = {
  dx9: [
    "d3dx9_24.dll", "d3dx9_25.dll", "d3dx9_26.dll", "d3dx9_27.dll", "d3dx9_28.dll",
    "d3dx9_29.dll", "d3dx9_30.dll", "d3dx9_31.dll", "d3dx9_32.dll", "d3dx9_33.dll",
    "d3dx9_34.dll", "d3dx9_35.dll", "d3dx9_36.dll", "d3dx9_37.dll", "d3dx9_38.dll",
    "d3dx9_39.dll", "d3dx9_40.dll", "d3dx9_41.dll", "d3dx9_42.dll", "d3dx9_43.dll",
  ],
  dx10: [
    "d3dx10.dll", "d3dx10_33.dll", "d3dx10_34.dll", "d3dx10_35.dll", "d3dx10_36.dll",
    "d3dx10_37.dll", "d3dx10_38.dll", "d3dx10_39.dll", "d3dx10_40.dll", "d3dx10_41.dll",
    "d3dx10_42.dll", "d3dx10_43.dll",
  ],
  dx11: ["d3dx11_42.dll", "d3dx11_43.dll"],
  shader: [
    "d3dcompiler_33.dll", "d3dcompiler_34.dll", "d3dcompiler_35.dll", "d3dcompiler_36.dll",
    "d3dcompiler_37.dll", "d3dcompiler_38.dll", "d3dcompiler_39.dll", "d3dcompiler_40.dll",
    "d3dcompiler_41.dll", "d3dcompiler_42.dll", "d3dcompiler_43.dll", "d3dcsx_42.dll",
    "d3dcsx_43.dll",
  ],
  audio: [
    "x3daudio1_0.dll", "x3daudio1_1.dll", "x3daudio1_2.dll", "x3daudio1_5.dll",
    "x3daudio1_6.dll", "x3daudio1_7.dll", "xactengine3_0.dll", "xactengine3_1.dll",
    "xactengine3_2.dll", "xactengine3_3.dll", "xactengine3_4.dll", "xactengine3_5.dll",
    "xactengine3_6.dll", "xactengine3_7.dll", "xapofx1_0.dll", "xapofx1_1.dll",
    "xapofx1_2.dll", "xapofx1_3.dll", "xapofx1_4.dll", "xapofx1_5.dll",
    "xaudio2_0.dll", "xaudio2_1.dll", "xaudio2_2.dll", "xaudio2_3.dll",
    "xaudio2_4.dll", "xaudio2_5.dll", "xaudio2_6.dll", "xaudio2_7.dll",
  ],
  input: ["xinput1_1.dll", "xinput1_2.dll", "xinput1_3.dll", "xinput9_1_0.dll"],
};

/// 预览用：每个架构下缺几个、异常几个（都是偶数，好平摊到 x86 / x64）
export const DEMO_STATE = {
  dx9: { missing: 8, bad: 2 },
  dx10: { missing: 6, bad: 2 },
  dx11: { missing: 2, bad: 0 },
  shader: { missing: 4, bad: 0 },
  audio: { missing: 16, bad: 2 },
  input: { missing: 0, bad: 0 },
};

/// 预览用：每组的文件总数（和 payload 里真实的对上，一共 184）
export const DEMO_TOTAL = { dx9: 40, dx10: 24, dx11: 4, shader: 26, audio: 82, input: 8 };

/// 检测。onLog 每查到一行调一次——界面上那屏"跑代码"就是它喂的。
export async function detect(onLog) {
  if (inTauri) return invoke("detect");

  const jobs = [];
  for (const g of GROUPS) {
    for (const name of DEMO_FILES[g.id] ?? []) {
      for (const arch of ["x86", "x64"]) jobs.push({ g, name, arch });
    }
  }

  let last = "";
  for (let i = 0; i < jobs.length; i++) {
    const { g, name, arch } = jobs[i];
    const list = DEMO_FILES[g.id] ?? [];
    const idx = list.indexOf(name);
    const missPerArch = (DEMO_STATE[g.id]?.missing ?? 0) / 2;
    const badPerArch = (DEMO_STATE[g.id]?.bad ?? 0) / 2;
    // 名单后面几个当"缺失"，再往前几个当"异常"
    const state =
      idx >= list.length - missPerArch
        ? "missing"
        : idx >= list.length - missPerArch - badPerArch
          ? "bad"
          : "ok";
    onLog?.({
      group: g.id === last ? "" : g.name,
      text: `${arch}\\${name}`,
      state,
      index: i,
      total: jobs.length,
    });
    last = g.id;
    await new Promise((r) => setTimeout(r, 4));
  }

  return GROUPS.map((g) => {
    const total = DEMO_TOTAL[g.id] ?? (DEMO_FILES[g.id] ?? []).length * 2;
    const missing = DEMO_STATE[g.id]?.missing ?? 0;
    const abnormal = DEMO_STATE[g.id]?.bad ?? 0;
    return {
      id: g.id,
      name: g.name,
      note: noteOf(missing, abnormal),
      total,
      missing,
      abnormal,
    };
  });
}

export function noteOf(missing, abnormal) {
  if (!missing && !abnormal) return "已齐全";
  if (!missing) return `异常 ${abnormal}`;
  if (!abnormal) return `缺 ${missing}`;
  return `缺 ${missing} · 异常 ${abnormal}`;
}

export function onScanLog(cb) {
  return inTauri
    ? listen("scan:log", (e) => cb(e.payload))
    : Promise.resolve(() => {});
}

/// 某个分组缺哪些文件（点开那一行时调）
export function missingFiles(id) {
  if (inTauri) return invoke("missing_files", { id });
  const names = DEMO_FILES[id] ?? [];
  const miss = (DEMO_STATE[id]?.missing ?? 0) / 2;
  const bad = (DEMO_STATE[id]?.bad ?? 0) / 2;
  const out = [];
  for (let i = 0; i < names.length; i++) {
    const name = names[i];
    if (i >= names.length - miss - bad && i < names.length - miss) {
      out.push({ name, x86: true, x64: true, why: "0 字节" });
    } else if (i >= names.length - miss) {
      out.push({ name, x86: true, x64: true, why: "" });
    }
  }
  return Promise.resolve(out);
}

/// rows = 界面上那几行的 id（后端自己展开成 x86 / x64）
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

/// 完成屏上的「立刻重启」：把那几个"重启后才替换"的文件落实下去
export function restartNow() {
  return inTauri ? invoke("restart_now") : Promise.resolve(false);
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
