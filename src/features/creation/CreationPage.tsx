import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router";
import { AssetLibrary } from "./AssetLibrary";
import { ThumbnailEditor } from "./thumbnails/ThumbnailEditor";
import { TextAssistant } from "./texts/TextAssistant";
import { MomentsTool } from "./moments/MomentsTool";

const TABS = ["thumbnails", "library", "texts", "moments"] as const;
type Tab = (typeof TABS)[number];

/** Creation tools: thumbnail editor, asset library, text assistant and moments. */
export function CreationPage() {
  const { t } = useTranslation();
  const [params, setParams] = useSearchParams();
  const tab = (TABS as readonly string[]).includes(params.get("tab") ?? "")
    ? (params.get("tab") as Tab)
    : "thumbnails";
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">{t("nav.creation")}</h1>
      <div className="flex gap-1" role="tablist">
        {TABS.map((k) => (
          <button
            key={k}
            role="tab"
            aria-selected={tab === k}
            className={`rounded-md px-3 py-1 text-sm ${tab === k ? "bg-[var(--accent)] text-white" : "border border-[var(--border)]"}`}
            onClick={() => setParams({ tab: k })}
          >
            {t(`creation.tab.${k}`)}
          </button>
        ))}
      </div>
      {tab === "thumbnails" && <ThumbnailEditor />}
      {tab === "library" && <AssetLibrary />}
      {tab === "texts" && <TextAssistant />}
      {tab === "moments" && <MomentsTool />}
    </div>
  );
}
