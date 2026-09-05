// React, Next.js, and Remix share TSX expression syntax.
type Props = { label: "app.type" };
export default function Component() {
  return <button title={t("app.live")}>{/* t("app.comment") */}{t(`app.${kind}`)}</button>;
}
