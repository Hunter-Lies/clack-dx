<script setup>
import { computed } from "vue";

// 机身上唯一那个键。跟着流程换说法：检测 -> 修复 N 个 -> 完成
const props = defineProps({
  /// idle | scan | report | busy | done | fail
  phase: { type: String, default: "idle" },
  count: { type: Number, default: 0 },
});

defineEmits(["press"]);

const text = computed(() => {
  switch (props.phase) {
    case "idle":
      return "检测";
    case "scan":
      return "检测中";
    case "busy":
      return "修复中";
    case "done":
      return "完成";
    case "fail":
      return "重试";
    default:
      return props.count > 0 ? `修复 ${props.count} 个` : "无需修复";
  }
});

const locked = computed(
  () => props.phase === "scan" || props.phase === "busy" || (props.phase === "report" && props.count === 0),
);
</script>

<template>
  <button class="install" :disabled="locked" @click="$emit('press')">
    {{ text }}
  </button>
</template>
