import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type AssistResult } from "../../../lib/bindings";
import { errorMessage } from "../../../lib/ipc";
import {
  BUILT_IN,
  LIMITS,
  generate,
  hashtags,
  type Field,
  type Template,
  type Vars,
} from "./templates";

const PLATFORMS = ["youtube", "tiktok", "instagram", "twitch"] as const;
const FIELDS: Field[] = ["title", "description"];
const field = "w-full rounded-md border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-sm";
const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs disabled:opacity-50";

function Copy({ text }: { text: string }) {
  const { t } = useTranslation();
  const [done, setDone] = useState(false);
  return (
    <button
      className={btn}
      onClick={() =>
        void navigator.clipboard.writeText(text).then(() => {
          setDone(true);
          setTimeout(() => setDone(false), 1500);
        })
      }
    >
      {done ? t("publish.copied") : t("texts.copy")}
    </button>
  );
}

function LlmPanel({ platform, vars }: { platform: string; vars: Partial<Vars> }) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const [key, setKey] = useState("");
  const status = useQuery({ queryKey: ["llmStatus"], queryFn: () => commands.llmStatus() });
  const saveKey = useMutation({
    mutationFn: () => commands.setLlmKey(key),
    onSuccess: () => {
      setKey("");
      void qc.invalidateQueries({ queryKey: ["llmStatus"] });
    },
  });
  const clear = useMutation({
    mutationFn: () => commands.clearLlmKey(),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["llmStatus"] }),
  });
  const notes = [
    vars.juego && `${t("texts.var.juego")}: ${vars.juego}`,
    vars.serie && `${t("texts.var.serie")}: ${vars.serie} ${vars.episodio ?? ""}`,
    vars.palabras && `${t("texts.var.palabras")}: ${vars.palabras}`,
  ]
    .filter(Boolean)
    .join("\n");
  const request = {
    platform,
    topic: vars.tema ?? "",
    notes,
    language: i18n.language === "en" ? "en" : "es",
  };
  const gen = useMutation<AssistResult>({ mutationFn: () => commands.llmGenerate(request) });

  return (
    <section className="space-y-2 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-3">
      <h2 className="text-sm font-medium">{t("texts.llmTitle")}</h2>
      {status.data ? (
        <>
          <p className="text-xs text-[var(--muted)]">{t("texts.llmWillSend")}</p>
          <pre className="whitespace-pre-wrap rounded-md bg-[var(--bg)] p-2 text-xs">
            {`${t("texts.var.tema")}: ${request.topic || "—"}${notes ? `\n${notes}` : ""}`}
          </pre>
          <div className="flex flex-wrap gap-2">
            <button
              className="rounded-md bg-[var(--accent)] px-3 py-1 text-xs text-white disabled:opacity-50"
              disabled={!request.topic.trim() || gen.isPending}
              onClick={() => gen.mutate()}
            >
              {gen.isPending ? t("texts.generating") : t("texts.generateAi")}
            </button>
            <button className={btn} onClick={() => clear.mutate()}>
              {t("texts.removeKey")}
            </button>
          </div>
          {gen.isError && <p className="text-xs text-red-500">{errorMessage(gen.error, t)}</p>}
          {gen.data && (
            <div className="space-y-2 text-sm">
              <ul className="space-y-1">
                {gen.data.titles.map((title) => (
                  <li key={title} className="flex items-center justify-between gap-2">
                    <span>{title}</span>
                    <Copy text={title} />
                  </li>
                ))}
              </ul>
              <div className="flex items-start justify-between gap-2">
                <p className="whitespace-pre-wrap text-xs">{gen.data.description}</p>
                <Copy text={gen.data.description} />
              </div>
              <div className="flex items-center justify-between gap-2">
                <p className="text-xs">{gen.data.hashtags.map((h) => `#${h}`).join(" ")}</p>
                <Copy text={gen.data.hashtags.map((h) => `#${h}`).join(" ")} />
              </div>
            </div>
          )}
        </>
      ) : (
        <>
          <p className="text-xs text-[var(--muted)]">{t("texts.llmHint")}</p>
          <div className="flex gap-2">
            <input
              type="password"
              className={field}
              placeholder="sk-ant-…"
              value={key}
              onChange={(e) => setKey(e.target.value)}
            />
            <button className={btn} disabled={!key.trim()} onClick={() => saveKey.mutate()}>
              {t("texts.saveKey")}
            </button>
          </div>
          {saveKey.isError && (
            <p className="text-xs text-red-500">{errorMessage(saveKey.error, t)}</p>
          )}
        </>
      )}
    </section>
  );
}

/** Titles, descriptions and hashtags from templates; optional help from an LLM. */
export function TextAssistant() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [platform, setPlatform] = useState<string>("youtube");
  const [vars, setVars] = useState<Partial<Vars>>({ fecha: new Date().toLocaleDateString() });
  const [newTpl, setNewTpl] = useState({
    name: "",
    field: "title" as Field,
    body: "",
    forAll: false,
  });
  const user = useQuery({
    queryKey: ["textTemplates"],
    queryFn: () => commands.listTextTemplates(),
  });
  const all: Template[] = [
    ...BUILT_IN,
    ...(user.data ?? []).map((u) => ({ ...u, field: u.field as Field })),
  ];
  const save = useMutation({
    mutationFn: () =>
      commands.saveTextTemplate({
        id: null,
        platform: newTpl.forAll ? null : platform,
        field: newTpl.field,
        name: newTpl.name,
        body: newTpl.body,
      }),
    onSuccess: () => {
      setNewTpl({ name: "", field: newTpl.field, body: "", forAll: false });
      void qc.invalidateQueries({ queryKey: ["textTemplates"] });
    },
  });
  const del = useMutation({
    mutationFn: (id: string) => commands.deleteTextTemplate(id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["textTemplates"] }),
  });
  const set = (k: keyof Vars, v: string) => setVars((x) => ({ ...x, [k]: v }));
  const tags = hashtags(vars, platform);
  const limits = LIMITS[platform];

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <div className="space-y-3">
        <div className="flex gap-1">
          {PLATFORMS.map((p) => (
            <button
              key={p}
              className={`rounded-md px-3 py-1 text-sm ${platform === p ? "bg-[var(--accent)] text-white" : "border border-[var(--border)]"}`}
              onClick={() => setPlatform(p)}
            >
              {t(`texts.platform.${p}`)}
            </button>
          ))}
        </div>
        <div className="grid grid-cols-2 gap-2">
          {(["tema", "juego", "serie", "episodio", "canal", "palabras"] as const).map((k) => (
            <label
              key={k}
              className={`space-y-0.5 text-xs ${k === "tema" || k === "palabras" ? "col-span-2" : ""}`}
            >
              <span className="text-[var(--muted)]">{t(`texts.var.${k}`)}</span>
              <input
                className={field}
                value={vars[k] ?? ""}
                onChange={(e) => set(k, e.target.value)}
              />
            </label>
          ))}
        </div>
        {FIELDS.map((f) => (
          <section key={f} className="space-y-1">
            <h3 className="text-sm font-medium">
              {t(`texts.field.${f}`)}
              {f === "title" && !limits.title && (
                <span className="ml-2 text-xs text-[var(--muted)]">{t("texts.noTitleHere")}</span>
              )}
            </h3>
            <ul className="space-y-1">
              {generate(all, f, platform, vars).map((r) => (
                <li
                  key={r.template.id}
                  className="flex items-start justify-between gap-2 rounded-md border border-[var(--border)] p-2 text-sm"
                >
                  <div className="min-w-0">
                    <p className="whitespace-pre-wrap">{r.text}</p>
                    <p className="text-xs text-[var(--muted)]">
                      {r.template.name}
                      {r.tooLong && <span className="ml-2 text-red-500">{t("texts.tooLong")}</span>}
                    </p>
                  </div>
                  <div className="flex gap-1">
                    <Copy text={r.text} />
                    {!r.template.builtIn && (
                      <button className={btn} onClick={() => del.mutate(r.template.id)}>
                        ✕
                      </button>
                    )}
                  </div>
                </li>
              ))}
            </ul>
          </section>
        ))}
        <section className="space-y-1">
          <h3 className="text-sm font-medium">{t("texts.field.hashtags")}</h3>
          <div className="flex items-center justify-between gap-2 rounded-md border border-[var(--border)] p-2 text-sm">
            <span>{tags.join(" ") || "—"}</span>
            <Copy text={tags.join(" ")} />
          </div>
          <p className="text-xs text-[var(--muted)]">
            {t("texts.hashtagRange", { min: limits.hashtags[0], max: limits.hashtags[1] })}
          </p>
        </section>
      </div>

      <div className="space-y-3">
        <LlmPanel platform={platform} vars={vars} />
        <section className="space-y-2 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-3">
          <h2 className="text-sm font-medium">{t("texts.newTemplate")}</h2>
          <p className="text-xs text-[var(--muted)]">{t("texts.placeholders")}</p>
          <div className="flex gap-2">
            <input
              className={field}
              placeholder={t("texts.templateName")}
              value={newTpl.name}
              onChange={(e) => setNewTpl({ ...newTpl, name: e.target.value })}
            />
            <select
              className={field}
              value={newTpl.field}
              onChange={(e) => setNewTpl({ ...newTpl, field: e.target.value as Field })}
            >
              {FIELDS.map((f) => (
                <option key={f} value={f}>
                  {t(`texts.field.${f}`)}
                </option>
              ))}
            </select>
          </div>
          <textarea
            className={`${field} min-h-20`}
            value={newTpl.body}
            placeholder="{{serie}} #{{episodio}} | {{tema}}"
            onChange={(e) => setNewTpl({ ...newTpl, body: e.target.value })}
          />
          <label className="flex items-center gap-1 text-xs">
            <input
              type="checkbox"
              checked={newTpl.forAll}
              onChange={(e) => setNewTpl({ ...newTpl, forAll: e.target.checked })}
            />
            {t("texts.allNetworks")}
          </label>
          <button
            className={btn}
            disabled={!newTpl.name.trim() || !newTpl.body.trim()}
            onClick={() => save.mutate()}
          >
            {t("texts.saveTemplate")}
          </button>
          {save.isError && <p className="text-xs text-red-500">{errorMessage(save.error, t)}</p>}
        </section>
      </div>
    </div>
  );
}
