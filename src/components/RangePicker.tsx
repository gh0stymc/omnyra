import { useTranslation } from "react-i18next";
import { RANGES } from "../lib/range";
import { useUi } from "../stores/ui";

/** Period selector shared by every screen (remembered between sessions). */
export function RangePicker() {
  const { t } = useTranslation();
  const { range, setRange } = useUi();
  return (
    <div
      className="inline-flex gap-0.5 rounded-xl border border-[var(--border)] bg-[var(--panel)] p-0.5"
      role="group"
      aria-label={t("dashboard.rangeLabel")}
    >
      {RANGES.map((r) => (
        <button
          key={r}
          aria-pressed={r === range}
          className={`rounded-[10px] px-2.5 py-1 text-xs font-medium ${r === range ? "bg-[var(--accent)] text-white" : "text-[var(--muted)] hover:text-[var(--text)]"}`}
          onClick={() => setRange(r)}
        >
          {t(`dashboard.range.${r}`)}
        </button>
      ))}
    </div>
  );
}
