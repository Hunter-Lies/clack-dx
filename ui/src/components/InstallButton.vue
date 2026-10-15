<script setup>
// 机身上唯一那个键。三种状态：待命（橙、可按）/ 进行中（灰、锁住）/ 完成（灰、可按 = 关掉）
const props = defineProps({
  state: { type: String, default: "idle" }, // idle | busy | done
});

defineEmits(["press"]);

const LABEL = {
  idle: "修复",
  busy: "修复中",
  done: "完成",
  fail: "重试",
};
</script>

<template>
  <button class="install" :disabled="state === 'busy'" @click="$emit('press')">
    {{ LABEL[state] ?? LABEL.idle }}
  </button>
</template>
