import { useTranslation } from "react-i18next";
import { RANGES } from "../lib/range";
import { useUi } from "../stores/ui";

/** Period selector shared by every screen (remembered between sessions). */
export function RangePicker() {
  const { t } = useTranslation();
  const { range, setRange } = useUi();
  return (
    <div className="flex gap-1" role="group" aria-label={t("dashboard.rangeLabel")}>
      {RANGES.map((r) => (
        <button
          key={r}
          aria-pressed={r === range}
          className={`rounded-md px-2 py-1 text-xs ${r === range ? "bg-[var(--accent)] text-white" : "border border-[var(--border)]"}`}
          onClick={() => setRange(r)}
        >
          {t(`dashboard.range.${r}`)}
        </button>
      ))}
    </div>
  );
}
