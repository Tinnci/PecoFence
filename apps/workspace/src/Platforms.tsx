import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Cpu, RefreshCw } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import type { FocusModel, Platform } from "./contracts/setup";
import { focus } from "./api";
import { queryClient } from "./focus-cache";
import { FailureNote, failureOf } from "./TemplateImport";
import { t } from "./i18n";

export const platformsKey = ["platforms"] as const;
export const platformsOptions = {
  queryKey: platformsKey,
  queryFn: async () => (await focus("platforms")).platforms ?? [],
  staleTime: 10 * 60_000,
};

/** One line for a platform: vendor · chip · Android · kernel, whatever is known. */
export function platformLabel(p: Platform | null | undefined) {
  if (!p) return "";
  return [
    p.vendor,
    p.chip,
    p.android.join("/"),
    p.kernel.length ? `kernel ${p.kernel.join("/")}` : "",
    p.baseband ? t("platform.basebandValue", { value: p.baseband }) : "",
  ]
    .filter(Boolean)
    .join(" · ");
}

async function savePlatform(
  code: string,
  values: { chip?: string; baseband?: string },
) {
  await focus("chip", { code, ...values });
  await queryClient.invalidateQueries({ queryKey: platformsKey });
  if (values.chip !== undefined)
    void queryClient.invalidateQueries({ queryKey: ["related"] });
}

function PlatformRefresh({
  busy,
  onRefresh,
}: {
  busy: boolean;
  onRefresh: () => void;
}) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      disabled={busy}
      aria-label={t("platform.refresh")}
      title={t("platform.refresh")}
      onClick={onRefresh}
    >
      {busy ? <Spinner aria-hidden="true" /> : <RefreshCw aria-hidden="true" />}
    </Button>
  );
}

/** Shares the same cached facts and edits as the home platform list. */
export function ModelPlatform({ code }: { code: string }) {
  useTranslation();
  const query = useQuery(platformsOptions, queryClient);
  const [editingCode, setEditingCode] = useState<string | null>(null);
  const platform = query.data?.find((p) => p.code === code);
  const label = platformLabel(platform);
  if (!platform && !query.error) return null;
  return (
    <>
      <div className="model-platform">
        {platform ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="platform-edit-trigger"
            aria-label={t("platform.editFor", { code })}
            title={platform.evidence.join("\n")}
            onClick={() => setEditingCode(code)}
          >
            <Cpu size={14} aria-hidden="true" />{" "}
            {label || t("platform.addDetails")}
          </Button>
        ) : null}
        <PlatformRefresh
          busy={query.isFetching}
          onRefresh={() => void query.refetch()}
        />
        {query.error ? (
          <span className="short" role="alert">
            {t("platform.loadFailed")}
          </span>
        ) : null}
      </div>
      {platform && editingCode === code ? (
        <PlatformEditor
          key={code}
          platform={platform}
          onClose={() => setEditingCode(null)}
        />
      ) : null}
    </>
  );
}

function PlatformEditor({
  platform,
  onClose,
}: {
  platform: Platform;
  onClose: () => void;
}) {
  const [initial] = useState({
    chip: platform.chip,
    baseband: platform.baseband ?? "",
  });
  const [chip, setChip] = useState(initial.chip);
  const [baseband, setBaseband] = useState(initial.baseband);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const values: { chip?: string; baseband?: string } = {};
  if (chip.trim() !== initial.chip) values.chip = chip.trim();
  if (baseband.trim() !== initial.baseband) values.baseband = baseband.trim();
  const invalid = [chip, baseband].some(
    (v) => [...v].length > 80 || /[\u0000-\u001f\u007f]/.test(v),
  );
  async function save() {
    if (busy || invalid || !Object.keys(values).length) return;
    setBusy(true);
    setFailed(false);
    try {
      await savePlatform(platform.code, values);
      onClose();
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent className="local-dialog">
        <DialogTitle>
          {t("platform.editTitle", { code: platform.code })}
        </DialogTitle>
        <DialogDescription>{t("platform.editHint")}</DialogDescription>
        <form
          className="platform-editor"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <label>
            {t("platform.chipColumn")}
            <Input
              autoFocus
              value={chip}
              maxLength={80}
              disabled={busy}
              onChange={(e) => setChip(e.target.value)}
            />
          </label>
          <label>
            {t("platform.basebandColumn")}
            <Input
              value={baseband}
              maxLength={80}
              disabled={busy}
              onChange={(e) => setBaseband(e.target.value)}
            />
          </label>
          {failed ? (
            <p className="short" role="alert">
              {t("platform.saveFailed")}
            </p>
          ) : null}
          <div className="local-row">
            <Button
              type="submit"
              disabled={busy || invalid || !Object.keys(values).length}
            >
              {busy ? <Spinner aria-hidden="true" /> : null}
              {t("views.save")}
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={onClose}
            >
              {t("model.cancel")}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** Every followed model's platform, grouped by SoC vendor; the chip is named here. */
export function Platforms({ models }: { models: FocusModel[] | null }) {
  useTranslation();
  const query = useQuery(platformsOptions, queryClient);
  const list = query.data;
  if (!list?.length && !query.error) return null;
  const name = (code: string) =>
    models?.find((m) => m.code === code)?.name ?? code;
  const groups = new Map<string, Platform[]>();
  for (const p of list ?? [])
    groups.set(p.vendor, [...(groups.get(p.vendor) ?? []), p]);
  const order = [...groups.keys()].sort((a, b) =>
    !a ? 1 : !b ? -1 : a.localeCompare(b),
  );
  return (
    <section className="platforms" aria-label={t("platform.title")}>
      <h2>
        <Cpu size={16} aria-hidden="true" />
        {t("platform.title")}
        <PlatformRefresh
          busy={query.isFetching}
          onRefresh={() => void query.refetch()}
        />
      </h2>
      <p className="quiet">{t("platform.hint")}</p>
      {query.error ? <FailureNote failure={failureOf(query.error)} /> : null}
      {order.map((vendor) => (
        <div key={vendor || "unknown"} className="platform-group">
          <h3>
            {vendor || t("platform.unknownVendor")}
            <span className="quiet"> {groups.get(vendor)!.length}</span>
          </h3>
          <div className="platform-columns" aria-hidden="true">
            <span>{t("platform.model")}</span>
            <span>{t("platform.system")}</span>
            <span>{t("platform.chipColumn")}</span>
            <span>{t("platform.basebandColumn")}</span>
          </div>
          <ul>
            {groups.get(vendor)!.map((p) => (
              <li key={p.code} title={p.evidence.join("\n")}>
                <strong>{name(p.code)}</strong>
                <span className="platform-facts">
                  {p.android.map((a) => (
                    <span key={a} className="related-term">
                      {a}
                    </span>
                  ))}
                  {p.kernel.map((k) => (
                    <span key={k} className="related-term">
                      kernel {k}
                    </span>
                  ))}
                </span>
                <PlatformField
                  key={`${p.code}:chip:${p.chip}`}
                  code={p.code}
                  field="chip"
                  saved={p.chip}
                />
                <PlatformField
                  key={`${p.code}:baseband:${p.baseband}`}
                  code={p.code}
                  field="baseband"
                  saved={p.baseband ?? ""}
                  suggestion={p.modem?.[0]}
                />
                {Boolean(p.modem?.length || p.modem_repos?.length) && (
                  <div className="platform-modem">
                    {p.modem?.length ? (
                      <span>
                        {t("platform.modemPackages")}: {p.modem.join(" · ")}
                      </span>
                    ) : null}
                    {p.modem_repos?.length ? (
                      <span>
                        {t("platform.modemSources")}:{" "}
                        {p.modem_repos.join(" · ")}
                      </span>
                    ) : null}
                    {p.modem_changes > 0 ? (
                      <span>
                        {t("platform.modemChanges", { count: p.modem_changes })}
                      </span>
                    ) : null}
                  </div>
                )}
              </li>
            ))}
          </ul>
        </div>
      ))}
    </section>
  );
}

function PlatformField({
  code,
  field,
  saved,
  suggestion,
}: {
  code: string;
  field: "chip" | "baseband";
  saved: string;
  /** What the code shows, offered while nothing is named. */
  suggestion?: string;
}) {
  const [value, setValue] = useState(saved);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  async function save(next = value.trim()) {
    if (busy || next === saved) return;
    setBusy(true);
    setFailed(false);
    try {
      await savePlatform(code, { [field]: next });
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="platform-field" aria-busy={busy}>
      <Input
        className={`platform-chip${failed ? " is-failed" : ""}`}
        value={value}
        maxLength={80}
        disabled={busy}
        placeholder={t(`platform.${field}Placeholder`)}
        aria-label={t(`platform.${field}`, { code })}
        aria-invalid={failed}
        title={failed ? t("platform.saveFailed") : undefined}
        onChange={(e) => setValue(e.target.value)}
        onBlur={() => void save()}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        }}
      />
      {suggestion && !saved && !value && !busy ? (
        <button
          type="button"
          className="platform-suggestion"
          title={t("platform.useSuggestion", { value: suggestion })}
          onClick={() => {
            setValue(suggestion);
            void save(suggestion);
          }}
        >
          {suggestion}
        </button>
      ) : null}
      {busy ? (
        <span className="quiet platform-field-status" role="status">
          {t("platform.saving")}
        </span>
      ) : null}
      {failed ? (
        <span className="short platform-field-status" role="alert">
          {t("platform.saveFailed")}
        </span>
      ) : null}
    </div>
  );
}
