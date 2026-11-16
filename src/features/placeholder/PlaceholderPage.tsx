import { useTranslation } from "react-i18next";

export function PlaceholderPage({ section }: { section: string }) {
  const { t } = useTranslation();
  return (
    <div>
      <h1 className="text-2xl font-semibold">{t(`nav.${section}`)}</h1>
      <p className="mt-2 text-[var(--muted)]">{t("placeholder.soon")}</p>
    </div>
  );
}
