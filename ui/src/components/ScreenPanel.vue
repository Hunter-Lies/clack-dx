<script setup>
// 屏幕本体：扫描线、渐晕、内阴影都在 console.css 的 .screen 里。
// 屏幕上不写标题和版号，只留内容槽。
</script>

<template>
  <div class="screen">
    <slot />
  </div>
</template>
