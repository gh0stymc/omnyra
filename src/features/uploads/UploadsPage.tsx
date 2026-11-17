import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router";
import { commands } from "../../lib/bindings";
import { queryKeys } from "../../lib/queryKeys";
import { UploadForm } from "./UploadForm";
import { UploadQueue } from "./UploadQueue";

export function UploadsPage() {
  const { t } = useTranslation();
  const [params] = useSearchParams();
  const accounts = useQuery({
    queryKey: queryKeys.accounts,
    queryFn: () => commands.listAccounts(),
  });
  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const canUpload = (platform: string) =>
    platforms.data?.find((p) => p.id === platform)?.capabilities.upload !== "none";
  const usable = (accounts.data ?? []).filter(
    (a) => a.status === "active" && canUpload(a.platform),
  );

  return (
    <div className="max-w-3xl space-y-6">
      <h1 className="text-2xl font-semibold">{t("upload.pageTitle")}</h1>
      {accounts.data && platforms.data && (
        <UploadForm
          // Remount when arriving from another account's "Upload video" button.
          key={params.get("account") ?? ""}
          accounts={usable}
          platforms={platforms.data}
          initialAccountId={params.get("account") ?? undefined}
        />
      )}
      <section className="space-y-2">
        <h2 className="font-medium">{t("upload.queueTitle")}</h2>
        <UploadQueue />
      </section>
    </div>
  );
}
