<script lang="ts">
// t("app.comment");
type Key = `app.${string}`;
const label = t("app.live");
</script>
<!-- {t("app.comment")} -->
<button title={t("app.attribute")}>{t("app.template")}</button>
<span>{t(`app.${kind}`)}</span>
<style>.label::after { content: "app.style"; }</style>
