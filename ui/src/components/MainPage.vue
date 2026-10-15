<script setup>
import { computed, nextTick, onMounted, ref, watch } from "vue";
import ScreenPanel from "./ScreenPanel.vue";
import InstallButton from "./InstallButton.vue";
import FinishMark from "./FinishMark.vue";
import { openExternal } from "../lib/link";
import { GROUPS, missingFiles } from "../lib/api";

const AUTHOR = "HunterLies";
const AUTHOR_URL = "https://space.bilibili.com/488494586";
const REPO_URL = "https://github.com/Hunter-Lies/clack-dx";

const props = defineProps({
  items: { type: Array, required: true },
  /// idle | scan | report | busy | done | fail
  phase: { type: String, default: "idle" },
  percent: { type: Number, default: 0 },
  failed: { type: Number, default: 0 },
  error: { type: String, default: "" },
  /// 修复过程中后端推的一句话（"解除占用 xxx.exe"）
  note: { type: String, default: "" },
  /// 被占用到杀不掉、安排成"重启后替换"的文件数
  pendingReboot: { type: Number, default: 0 },
  missingTotal: { type: Number, default: 0 },
  fileTotal: { type: Number, default: 0 },
  logLines: { type: Array, default: () => [] },
  version: { type: String, default: "" },
});

defineEmits(["press", "quit"]);

const allGood = computed(() => props.missingTotal === 0);
const seen = computed(() => props.phase === "report" || props.phase === "busy" || props.phase === "fail");

/// 屏幕底部那行状态：六步各有各的说法
const STATUS = {
  idle: "未检测",
  scan: "正在检测",
  report: "就绪",
  busy: "正在修复",
  done: "修复完成",
  fail: "有文件没补上",
};
const status = computed(() => {
  if (props.phase === "fail" && props.failed) {
    const base = `${props.failed} 组没补全`;
    return props.error ? `${base} · ${props.error}` : base;
  }
  // 解除占用的时候把话说明白，别只显示"正在修复"
  if (props.phase === "busy" && props.note) return props.note;
  return STATUS[props.phase] ?? STATUS.idle;
});

/// 屏幕上半部分那句大字
const verdict = computed(() => {
  if (props.phase === "idle") return "未检测";
  if (props.phase === "scan") return `${Math.round(props.percent)}%`;
  if (props.phase === "busy") return `${Math.round(props.percent)}%`;
  if (props.phase === "done") return "已补齐";
  return allGood.value ? "系统齐全" : `缺 ${props.missingTotal} 个`;
});

// 点一行展开：看这一组到底缺哪些文件
const openId = ref("");
const detail = ref([]);
const loading = ref(false);

async function toggle(it) {
  if (props.phase !== "report" || it.installed) return;
  if (openId.value === it.id) {
    openId.value = "";
    return;
  }
  openId.value = it.id;
  detail.value = [];
  loading.value = true;
  const list = await missingFiles(it.id);
  if (openId.value === it.id) {
    detail.value = list;
    loading.value = false;
  }
}

watch(
  () => props.phase,
  (p) => {
    if (p !== "report") openId.value = "";
  },
);

// 检测那屏：日志一行行往下滚，始终盯着最后一行
const logEl = ref(null);
async function stickToBottom() {
  await nextTick();
  const el = logEl.value;
  if (el) el.scrollTop = el.scrollHeight;
}
watch(() => props.logLines.length, stickToBottom);
onMounted(stickToBottom);

// 清单区底部渐隐只在真的装不下时出现
const rowsEl = ref(null);
const scrollable = ref(false);
async function checkFit() {
  await nextTick();
  const el = rowsEl.value;
  scrollable.value = !!el && el.scrollHeight > el.clientHeight + 1;
}
onMounted(checkFit);
watch(() => props.items.length, checkFit);
watch(() => props.phase, checkFit);
watch(openId, checkFit);

// 调 UI 用：?open=dx9 直接展开那一行（只在 dev 生效）
onMounted(() => {
  const id = window.__openRow;
  if (!id) return;
  const it = props.items.find((x) => x.id === id);
  if (it) toggle(it);
});

// 调 UI 用：?debug=1 把各块实测高度写进 DOM（headless 取 DOM 时看）
const dbg = ref("");
if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("debug")) {
  onMounted(async () => {
    await nextTick();
    const r = (sel) => {
      const b = document.querySelector(sel)?.getBoundingClientRect();
      return b ? `${Math.round(b.top)}..${Math.round(b.bottom)}` : "-";
    };
    dbg.value = `DEBUG console=${r(".console")} page=${r(".page")} screen=${r(".screen")} deck=${r(".deck")} status=${r(".screen-status")} idlebox=${r(".idle-pane")} skel=${r(".skeleton")} rows=${r(".rows")} card=${r(".card")} install=${r(".install")}`;
  });
}
</script>

<template>
  <div class="page">
    <div v-if="dbg" class="dbg">{{ dbg }}</div>
    <!-- 左：整块屏幕留给报告 -->
    <ScreenPanel>
      <!-- 修完只留一个圈打勾 -->
      <template v-if="phase === 'done'">
        <FinishMark />
        <p v-if="pendingReboot > 0" class="after">
          {{ pendingReboot }} 个文件正被占用，已安排重启后替换
        </p>
      </template>

      <!-- 一、还没检测 -->
      <template v-else-if="phase === 'idle'">
        <div class="idle-pane">
          <span class="big">未检测</span>
          <span class="hint">按下面的检测键，先看看缺什么</span>
        </div>
        <div class="skeleton">
          <div v-for="g in GROUPS" :key="g.id" class="sk">
            <i></i>
            <span>{{ g.name }}</span>
            <b>—</b>
          </div>
        </div>
      </template>

      <!-- 二、正在检测：像跑代码一样一行行往下吐 -->
      <template v-else-if="phase === 'scan'">
        <div class="head">
          <span class="cap">正在检测</span>
          <span class="cap-en">SCANNING</span>
        </div>
        <div ref="logEl" class="log">
          <div
            v-for="(l, i) in logLines"
            :key="i"
            class="l"
            :class="{ head: l.head, bad: l.missing && !l.head }"
          >
            <span class="t">{{ l.text }}</span>
            <span v-if="!l.head" class="f">{{ l.missing ? "缺失" : "ok" }}</span>
          </div>
        </div>
      </template>

      <!-- 三、报告 / 修复中 -->
      <template v-else>
        <div class="head">
          <span class="cap">检测结果</span>
          <span class="cap-en">DIRECTX RUNTIME</span>
        </div>

        <div class="verdict" :class="{ good: allGood && phase !== 'busy' }">
          <span class="big">{{ verdict }}</span>
          <span class="sub">
            {{ fileTotal }} 个文件 · {{ items.length }} 组<template v-if="allGood && seen && phase !== 'busy'">，无需修复</template>
          </span>
        </div>

        <div ref="rowsEl" class="rows" :class="{ scrollable }">
          <template v-for="it in items" :key="it.id">
            <button
              class="row"
              :class="{ run: it.running, done: it.installed, open: openId === it.id }"
              @click="toggle(it)"
            >
              <i class="dot"></i>
              <span class="nm">{{ it.nm }}</span>
              <span class="cnt">{{ it.installed ? "已齐全" : it.ver }}</span>
              <svg
                v-if="!it.installed"
                class="chev"
                width="9"
                height="9"
                viewBox="0 0 9 9"
                aria-hidden="true"
              >
                <path
                  d="M3 1.4 6.3 4.5 3 7.6"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.4"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            </button>

            <div v-if="openId === it.id" class="detail">
              <span v-if="loading" class="file wait">读取中…</span>
              <span v-for="f in detail" :key="f.name" class="file">
                {{ f.name }}<i>{{ f.x86 && f.x64 ? "32/64" : f.x86 ? "32" : "64" }}</i>
              </span>
            </div>
          </template>
        </div>
      </template>

      <!-- 屏幕底部状态行 -->
      <div
        v-if="phase !== 'done'"
        class="screen-status"
        :class="{ run: phase === 'scan' || phase === 'busy', good: seen && allGood }"
      >
        <i class="led"></i>
        <span class="word">{{ status }}</span>
        <span class="bar"><i :style="{ width: percent + '%' }"></i></span>
        <span class="pct">{{ Math.round(percent) }}%</span>
      </div>
    </ScreenPanel>

    <!-- 右：机身面板。铭牌在最上，厂标和那颗大键压在底下 -->
    <div class="deck">
      <div class="plate">
        <span class="mark">咔哒</span>
        <span class="latin">Clack DX</span>
        <span class="ver">{{ version }}</span>
        <button
          class="xbtn"
          :disabled="phase === 'busy' || phase === 'scan'"
          :title="phase === 'busy' || phase === 'scan' ? '先别关' : '关闭'"
          @mousedown.stop.prevent="$emit('quit')"
          @click.stop="$emit('quit')"
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

      <!-- 中间：三行数据 -->
      <div class="mid">
        <div class="stat"><span>分组</span><b>{{ items.length || GROUPS.length }}</b></div>
        <div class="stat"><span>文件</span><b>{{ fileTotal }}</b></div>
        <div class="stat">
          <span>待补</span><b :class="{ hot: missingTotal > 0 }">{{ missingTotal }}</b>
        </div>
      </div>

      <!-- 作者 + 跳转合在一张卡片里：上面一行署名，下面一排键 -->
      <div class="card who">
        <div class="line">
          <span class="k">作者</span>
          <b>{{ AUTHOR }}</b>
        </div>
        <div class="links">
          <button class="jump" :title="AUTHOR_URL" @click="openExternal(AUTHOR_URL)">
            <svg width="13" height="13" viewBox="0 0 12 12" aria-hidden="true">
              <path
                d="M4.7 2.3H2.5A1.3 1.3 0 0 0 1.2 3.6v5.9a1.3 1.3 0 0 0 1.3 1.3h5.9a1.3 1.3 0 0 0 1.3-1.3V7.3"
                stroke="currentColor"
                stroke-width="1.3"
                stroke-linecap="round"
              />
              <path
                d="M7.3 1.1h3.6v3.6M10.7 1.3 5.5 6.5"
                stroke="currentColor"
                stroke-width="1.3"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
            哔哩哔哩
          </button>
          <button class="jump" :title="REPO_URL" @click="openExternal(REPO_URL)">
            <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true">
              <path
                fill="currentColor"
                d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
              />
            </svg>
            GitHub
          </button>
        </div>
      </div>

      <InstallButton :phase="phase" :count="missingTotal" @press="$emit('press')" />
    </div>
  </div>
</template>
