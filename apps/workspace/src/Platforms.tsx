import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Cpu } from "lucide-react";
import { Input } from "@/components/ui/input";
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

/** Shares the same cached facts and edits as the home platform list. */
export function ModelPlatform({ code }: { code: string }) {
  useTranslation();
  const query = useQuery(platformsOptions, queryClient);
  const platform = query.data?.find((p) => p.code === code);
  const label = platformLabel(platform);
  return label ? (
    <p className="model-platform" title={platform?.evidence.join("\n")}>
      <Cpu size={14} aria-hidden="true" /> {label}
    </p>
  ) : null;
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
}: {
  code: string;
  field: "chip" | "baseband";
  saved: string;
}) {
  const [value, setValue] = useState(saved);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  async function save() {
    const next = value.trim();
    if (next === saved) return;
    setBusy(true);
    setFailed(false);
    try {
      await focus("chip", { code, [field]: next });
      // Read the latest projection after partial writes, including another field's save.
      await queryClient.invalidateQueries({ queryKey: platformsKey });
      if (field === "chip")
        void queryClient.invalidateQueries({ queryKey: ["related"] });
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  }
  return (
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
  );
}
