<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import MainPage from "./components/MainPage.vue";
import { setTheme } from "./lib/theme";
import { closeWindow, inTauri, startDrag } from "./lib/window";
import {
  installComponents,
  isElevated,
  onDone,
  onProgress,
  quitApp,
  requestElevation,
  scan,
} from "./lib/api";

const VERSION = "v1.0.0";
const STEP_MS = 620;

// 浏览器里单独调 UI（pnpm dev）时用的占位数据。跑在 Tauri 里会被 scan() 的结果替掉。
// 分组和 src-tauri/src/lib.rs 里的 GROUPS 对应。
const DEMO = [
  { id: "dx9", nm: "DirectX 9 运行库", ver: "缺 20 个" },
  { id: "dx10", nm: "DirectX 10 运行库", ver: "缺 13 个" },
  { id: "dx11", nm: "DirectX 11 运行库", ver: "缺 2 个" },
  { id: "shader", nm: "着色器编译器 D3DCompiler", ver: "缺 4 个" },
  { id: "audio", nm: "XAudio / XACT 音频", ver: "缺 33 个" },
  { id: "input", nm: "XInput 手柄支持", ver: "已齐全" },
];

const items = ref(DEMO.map((d) => ({ ...d, installed: false, running: false })));
const state = ref("idle"); // idle | busy | done | fail
const percent = ref(0);
const failed = ref(0);
const lastError = ref("");

let timer = 0;
let offs = [];

const percentText = computed(() => Math.round(percent.value));

function mark(id, patch) {
  const it = items.value.find((x) => x.id === id);
  if (it) Object.assign(it, patch);
}

// 调 UI 用：?state=busy&at=5 / ?state=done / ?theme=light，打包后这段会被摇掉
if (import.meta.env.DEV) {
  const q = new URLSearchParams(location.search);
  const s = q.get("state");
  if (s === "done") {
    state.value = "done";
    percent.value = 100;
    items.value.forEach((i) => (i.installed = true));
  } else if (s) {
    state.value = "busy";
    const at = Number(q.get("at") ?? 5);
    items.value.forEach((it, i) => {
      it.installed = i < at;
      it.running = i === at;
    });
    percent.value = (at / items.value.length) * 100;
  }
  if (q.get("theme")) setTheme(q.get("theme"));
}

// ---------- 假数据那一路：浏览器预览用，不联后端 ----------
function demoRun() {
  const ids = items.value.filter((i) => !i.installed);
  if (!ids.length) {
    state.value = "done";
    percent.value = 100;
    return;
  }
  state.value = "busy";
  percent.value = 0;
  let i = 0;
  const tick = () => {
    if (i >= ids.length) {
      state.value = "done";
      percent.value = 100;
      return;
    }
    const cur = ids[i];
    cur.running = true;
    percent.value = (i / ids.length) * 100;
    timer = setTimeout(() => {
      cur.running = false;
      cur.installed = true;
      i += 1;
      tick();
    }, STEP_MS);
  };
  tick();
}

// ---------- 真后端那一路 ----------
async function refresh() {
  const list = await scan();
  if (!list.length) return;
  items.value = list.map((c) => ({
    id: c.id,
    nm: c.name,
    ver: c.note,
    // 已齐全的那些行直接标成完成：按下去也不会再动它们
    installed: c.missing === 0,
    running: false,
  }));
}

function handleProgress(p) {
  if (p.state === "run") {
    mark(p.row, { running: true });
    percent.value = p.total ? (p.index / p.total) * 100 : 0;
    return;
  }
  mark(p.row, { running: false, installed: p.state === "done" });
  if (p.state === "fail") {
    failed.value += 1;
    lastError.value = p.message ?? "";
    // 具体是哪一个、什么原因，留给开发时看控制台
    console.warn("装不上:", p.id, p.message);
  }
  // 百分比按后端给的"这次要跑几个安装器"算：一行产品可能含 x86 + x64 两个，
  // 拿行数当分母会超过 100%（之前跑到过 140%）
  percent.value = p.total ? ((p.index + 1) / p.total) * 100 : 0;
}

function handleDone(d) {
  percent.value = 100;
  if (d.ok) {
    state.value = "done";
  } else {
    // 有装不上的就不给"完成"那一屏，停在清单上，按一下按钮能重试
    state.value = "fail";
  }
}

async function start() {
  clearTimeout(timer);
  if (state.value === "busy") return;

  if (!inTauri) return demoRun();

  const ids = items.value.filter((i) => !i.installed).map((i) => i.id);
  // 一个都不缺就直接收工——这种情况没必要弹 UAC
  if (!ids.length) {
    state.value = "done";
    percent.value = 100;
    return;
  }

  // 往系统目录写文件必须管理员权限。启动时已经拉过一次，这里是兜底：
  // 用户在 UAC 上点了"否"的话，再给一次机会。
  if (!(await isElevated()) && (await requestElevation(true))) return;

  failed.value = 0;
  lastError.value = "";
  percent.value = 0;
  state.value = "busy";
  await installComponents(ids);
}

function press() {
  if (state.value === "busy") return;
  if (state.value === "done") {
    // 装完那一刻按钮是「完成」，按下去关窗口。
    // 浏览器预览里没有窗口可关，退回待命，方便反复看完成动画。
    if (inTauri) closeWindow();
    else {
      state.value = "idle";
      percent.value = 0;
      items.value.forEach((i) => (i.installed = false));
    }
    return;
  }
  start();
}

// 关闭键走 mousedown 而不是 click：安装器会抢焦点，那一下点击可能只用来激活窗口、
// 不会变成 click 事件。按下就动手，用户不会觉得"点了没反应"。
// 再挂个兜底：正常关窗口没生效就直接结束进程。
let quitting = false;
function quit() {
  // 安装进行中不给关：后端也会挡（Alt+F4 / 任务栏关闭同样进不来）
  if (quitting || !inTauri || state.value === "busy") return;
  quitting = true;
  closeWindow();
  setTimeout(quitApp, 500);
}

// 机身任意处按下就拖窗口（含屏幕那块，列表靠滚轮滚，不占鼠标左键）。
// 只有真正的交互元素要排除，否则一点它们就变成拖窗口。
// 光靠 data-tauri-drag-region 只能拖那条 26px 的顶栏，用户找不到地方下手。
const NO_DRAG = ".install, .jump, .xbtn, button, a, input";
function onShellDown(e) {
  if (e.button !== 0) return;
  if (e.target.closest?.(NO_DRAG)) return;
  startDrag();
}

onMounted(async () => {
  if (!inTauri) return;
  offs = [await onProgress(handleProgress), await onDone(handleDone)];
  await refresh();
  // 不是管理员就用 runas 重新拉起自己，当前窗口退场，用户不需要点两次
  if (await requestElevation()) closeWindow();
});

onBeforeUnmount(() => {
  clearTimeout(timer);
  offs.forEach((off) => off && off());
});
</script>

<template>
  <div class="stage">
    <div class="console" :class="state" @mousedown="onShellDown">
      <div class="plate">
        <span class="mark">咔哒</span>
        <span class="latin">Clack DX</span>
        <span class="ver">{{ VERSION }}</span>
        <button
          class="xbtn"
          :disabled="state === 'busy'"
          :title="state === 'busy' ? '修复中，先别关' : '关闭'"
          @mousedown.stop.prevent="quit"
          @click.stop="quit"
        >
          <svg width="11" height="11" viewBox="0 0 11 11" aria-hidden="true">
            <path
              d="M1 1 10 10M10 1 1 10"
              stroke="currentColor"
              stroke-width="1.7"
              stroke-linecap="round"
            />
          </svg>
        </button>
      </div>

      <MainPage
        :items="items"
        :state="state"
        :percent="percentText"
        :failed="failed"
        :error="lastError"
        @press="press"
      />
    </div>
  </div>
</template>
