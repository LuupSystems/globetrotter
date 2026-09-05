// t("app.comment");
/* t(`app.${comment}`); */
type Key = `app.${string}`;
type Literal = "app.type";
const url = `app.${kind}`;
const pattern = /app.regex/;
const live = t("app.live");
const key = "app.literal" as const;
const dynamic = t(`app.${kind}`);
