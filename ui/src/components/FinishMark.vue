<script setup>
// 安装完成：屏幕上清空，只留一个圆圈打勾。
// 圈和勾都是描出来的，勾用 stroke-dashoffset 走一遍，收尾才有"落定"的感觉。
</script>

<template>
  <div class="finish">
    <svg class="mark" viewBox="0 0 48 48" aria-hidden="true">
      <circle class="ring" cx="24" cy="24" r="20.8" />
      <path class="tick" d="M15 25.4 21.4 31.8 33.4 17.6" />
    </svg>
  </div>
</template>
