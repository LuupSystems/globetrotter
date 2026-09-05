<script setup lang="ts">
// t("app.comment");
type Key = `app.${string}`;
const label = t("app.live");
const unrelated = `app.${kind}`;
</script>
<template>
  <!-- {{ t("app.comment") }} -->
  <button :title="t('app.attribute')">{{ t("app.template") }}</button>
  <span>{{ t(`app.${kind}`) }}</span>
</template>
<style scoped>
/* t("app.comment"); */
.label::after { content: "app.style"; }
</style>
