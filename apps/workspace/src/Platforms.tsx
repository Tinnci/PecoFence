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
    p.android[0],
    p.kernel.length ? `kernel ${p.kernel.join("/")}` : "",
  ]
    .filter(Boolean)
    .join(" · ");
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
                <ChipField code={p.code} chip={p.chip} />
              </li>
            ))}
          </ul>
        </div>
      ))}
    </section>
  );
}

function ChipField({ code, chip }: { code: string; chip: string }) {
  const [value, setValue] = useState(chip);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  async function save() {
    const next = value.trim();
    if (next === chip) return;
    setBusy(true);
    setFailed(false);
    try {
      const data = await focus("chip", { code, chip: next });
      if (data.platforms)
        queryClient.setQueryData(platformsKey, data.platforms);
      // Rankings weigh shared chips: let open suggestions read again.
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
      placeholder={t("platform.chipPlaceholder")}
      aria-label={t("platform.chip", { code })}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => void save()}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
      }}
    />
  );
}
