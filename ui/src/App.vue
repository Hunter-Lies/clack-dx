<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import MainPage from "./components/MainPage.vue";
import { setTheme } from "./lib/theme";
import { closeWindow, inTauri, startDrag } from "./lib/window";
import {
  GROUPS,
  DEMO_STATE,
  DEMO_TOTAL,
  noteOf,
  detect,
  installComponents,
  isElevated,
  onDone,
  onProgress,
  onScanLog,
  quitApp,
  requestElevation,
} from "./lib/api";

const VERSION = "v1.1.1";

// 界面一共六步：待检测 -> 检测中 -> 报告 -> 修复中 -> 完成（fail 是修复没干净）
const phase = ref("idle"); // idle | scan | report | busy | done | fail
const items = ref([]);
const logLines = ref([]);
const percent = ref(0);
const failed = ref(0);
const lastError = ref("");
/// 修复过程中后端推的一句话（"解除占用 xxx.exe" 这种），显示在状态行上
const note = ref("");
/// 被占用到杀不掉、只能重启后替换的文件数
const pendingReboot = ref(0);
/// 修完的交代：补了几个、花了多久
const copied = ref(0);
/// 覆盖回去的异常文件（损坏 / 旧版）
const repaired = ref(0);
const elapsed = ref(0);
let startedAt = 0;

let offs = [];

const percentText = computed(() => Math.round(percent.value));
const missingTotal = computed(() =>
  items.value.reduce(
    (n, i) => n + (i.installed ? 0 : (i.missing ?? 0) + (i.abnormal ?? 0)),
    0,
  ),
);
const fileTotal = computed(() => items.value.reduce((n, i) => n + (i.total ?? 0), 0));

function mark(id, patch) {
  const it = items.value.find((x) => x.id === id);
  if (it) Object.assign(it, patch);
}

/// 检测时后端一行行往外吐，这里照单收下（浏览器预览走 detect 的回调，Tauri 走事件）
function pushLog(p) {
  if (p.group) logLines.value.push({ head: true, text: p.group });
  logLines.value.push({ text: p.text, missing: p.missing });
  percent.value = p.total ? ((p.index + 1) / p.total) * 100 : 0;
}

function fillRows(rows) {
  items.value = rows.map((c) => ({
    id: c.id,
    nm: c.name,
    ver: c.note,
    // 又没缺又没异常的，直接标成完成：按下去也不会再动它们
    installed: c.missing === 0 && (c.abnormal ?? 0) === 0,
    missing: c.missing,
    abnormal: c.abnormal ?? 0,
    total: c.total,
    running: false,
  }));
}

// ---------- 调 UI 用的一些开关（打包后这段会被摇掉） ----------
if (import.meta.env.DEV) {
  const q = new URLSearchParams(location.search);
  if (q.get("theme")) setTheme(q.get("theme"));
  // 预展开某一行：headless 截图要拍明细面板的样子
  if (q.get("open")) window.__openRow = q.get("open");

  const demoRows = () =>
    GROUPS.map((g) => {
      const missing = DEMO_STATE[g.id]?.missing ?? 0;
      const abnormal = DEMO_STATE[g.id]?.bad ?? 0;
      return {
        id: g.id,
        name: g.name,
        note: noteOf(missing, abnormal),
        total: DEMO_TOTAL[g.id] ?? 40,
        missing,
        abnormal,
      };
    });

  if (q.get("phase") === "scan") {
    phase.value = "scan";
    const names = ["d3dx9_24.dll", "d3dx9_25.dll", "d3dx9_26.dll", "d3dx9_27.dll", "d3dx9_28.dll"];
    let n = 0;
    for (const g of GROUPS) {
      logLines.value.push({ head: true, text: g.name });
      for (const name of names) {
        for (const arch of ["x86", "x64"]) {
          logLines.value.push({ text: `${arch}\\${name}`, missing: n % 3 === 0 });
          n++;
        }
      }
    }
    percent.value = 46;
  } else if (q.get("phase") === "report") {
    phase.value = "report";
    fillRows(demoRows());
  } else if (q.get("state") === "done") {
    fillRows(demoRows());
    items.value.forEach((it) => (it.installed = true));
    phase.value = "done";
    percent.value = 100;
    copied.value = Number(q.get("copied") ?? 6);
    repaired.value = Number(q.get("repaired") ?? 0);
    elapsed.value = Number(q.get("secs") ?? 1.4);
    pendingReboot.value = Number(q.get("pending") ?? 0);
  } else if (q.get("state") === "busy") {
    phase.value = "busy";
    fillRows(demoRows());
    note.value = q.get("note") ?? "x86\\d3dx9_43.dll";
    const at = Number(q.get("at") ?? 3);
    items.value.forEach((it, i) => {
      it.installed = i < at;
      it.running = i === at;
    });
    percent.value = (at / items.value.length) * 100;
  }
}

// ---------- 主流程 ----------

async function runDetect() {
  logLines.value = [];
  percent.value = 0;
  phase.value = "scan";
  const rows = await detect(pushLog);
  fillRows(rows);
  percent.value = 0;
  phase.value = "report";
}

async function runRepair() {
  const ids = items.value.filter((i) => !i.installed).map((i) => i.id);
  if (!ids.length) {
    percent.value = 100;
    phase.value = "done";
    return;
  }

  // 往系统目录写文件必须管理员权限。启动时已经拉过一次，这里是兜底：
  // 用户在 UAC 上点了"否"的话，再给一次机会。
  if (!(await isElevated()) && (await requestElevation(true))) return;

  failed.value = 0;
  lastError.value = "";
  note.value = "";
  pendingReboot.value = 0;
  copied.value = 0;
  elapsed.value = 0;
  percent.value = 0;
  phase.value = "busy";
  startedAt = performance.now();
  await installComponents(ids);
}

async function start() {
  if (phase.value === "scan" || phase.value === "busy") return;

  if (phase.value === "done") {
    // 装完那一刻按钮是「完成」，按下去关窗口；浏览器预览里退回待检测，方便反复看
    if (inTauri) closeWindow();
    else {
      phase.value = "idle";
      items.value = [];
      percent.value = 0;
    }
    return;
  }

  if (phase.value === "idle") return runDetect();
  return runRepair();
}

// ---------- 后端事件 ----------

function handleProgress(p) {
  if (p.message) note.value = p.message;
  if (p.state === "run") {
    mark(p.row, { running: true });
    percent.value = p.total ? (p.index / p.total) * 100 : 0;
    return;
  }
  mark(p.row, { running: false, installed: p.state === "done" });
  if (p.state === "fail") {
    failed.value += 1;
    lastError.value = p.message ?? "";
    console.warn("没补上:", p.row, p.message);
  }
  percent.value = p.total ? ((p.index + 1) / p.total) * 100 : 0;
}

function handleDone(d) {
  percent.value = 100;
  pendingReboot.value = d.pending_reboot ?? 0;
  copied.value = d.copied ?? 0;
  repaired.value = d.repaired ?? 0;
  elapsed.value = startedAt ? Math.round((performance.now() - startedAt) / 100) / 10 : 0;
  // 别立刻跳走：让用户看清最后几行变成"已齐全"、状态行收尾
  setTimeout(() => {
    note.value = "";
    phase.value = d.ok ? "done" : "fail";
  }, 900);
}

// 关闭键走 mousedown 而不是 click：别的程序会抢焦点，那一下点击可能只用来激活窗口。
// 再挂个兜底：正常关窗口没生效就直接结束进程。
let quitting = false;
function quit() {
  if (quitting || !inTauri || phase.value === "busy" || phase.value === "scan") return;
  quitting = true;
  closeWindow();
  setTimeout(quitApp, 500);
}

// 机身任意处按下就拖窗口。只有真正的交互元素要排除，否则一点它们就变成拖窗口。
const NO_DRAG = ".install, .jump, .xbtn, button, a, input";
function onShellDown(e) {
  if (e.button !== 0) return;
  if (e.target.closest?.(NO_DRAG)) return;
  startDrag();
}

onMounted(async () => {
  if (!inTauri) return;
  offs = [
    await onScanLog(pushLog),
    await onProgress(handleProgress),
    await onDone(handleDone),
  ];
  // 不是管理员就用 runas 重新拉起自己，当前窗口退场，用户不需要点两次
  if (await requestElevation()) closeWindow();
});

onBeforeUnmount(() => {
  offs.forEach((off) => off && off());
});
</script>

<template>
  <div class="stage">
    <div class="console" @mousedown="onShellDown">
      <MainPage
        :items="items"
        :phase="phase"
        :percent="percentText"
        :failed="failed"
        :error="lastError"
        :note="note"
        :pending-reboot="pendingReboot"
        :copied="copied"
        :repaired="repaired"
        :elapsed="elapsed"
        :missing-total="missingTotal"
        :file-total="fileTotal"
        :log-lines="logLines"
        :version="VERSION"
        @press="start"
        @quit="quit"
      />
    </div>
  </div>
</template>
