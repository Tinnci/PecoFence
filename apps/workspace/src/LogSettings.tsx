import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ArrowLeft, ChevronDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { loggingSettings } from "./api";
import { FailureNote, failureOf } from "./TemplateImport";
import type { LogPolicy, LoggingSettings } from "./contracts/setup";
import { focusKey, queryClient } from "./focus-cache";
import { t } from "./i18n";

const settingsKey = [...focusKey, "logging-settings"];
type Field = Exclude<keyof LogPolicy, "schema_version">;
const limits: Record<Field, number> = {
  retention_days: 365,
  max_total_mib: 4096,
  daily_info_mib: 256,
  daily_max_mib: 512,
  daemon_file_mib: 256,
  daemon_files_per_stream: 100,
};
const normal: Field[] = ["retention_days", "max_total_mib"];
const advanced: Field[] = [
  "daily_info_mib",
  "daily_max_mib",
  "daemon_file_mib",
  "daemon_files_per_stream",
];

function PolicyForm({
  settings,
  busy,
  onSave,
  onReset,
}: {
  settings: LoggingSettings;
  busy: boolean;
  onSave: (policy: LogPolicy) => void;
  onReset: () => void;
}) {
  const [values, setValues] = useState(
    () =>
      Object.fromEntries(
        Object.keys(limits).map((key) => [
          key,
          String(settings.policy[key as Field]),
        ]),
      ) as Record<Field, string>,
  );
  const [expanded, setExpanded] = useState(false);
  const policy = {
    schema_version: 1,
    ...Object.fromEntries(
      Object.keys(limits).map((key) => [key, Number(values[key as Field])]),
    ),
  } as LogPolicy;
  const fieldsValid = Object.entries(limits).every(
    ([key, max]) =>
      values[key as Field].trim() !== "" &&
      Number.isInteger(policy[key as Field]) &&
      policy[key as Field] >= 1 &&
      policy[key as Field] <= max,
  );
  const ordered =
    policy.daily_info_mib <= policy.daily_max_mib &&
    policy.daily_max_mib <= policy.max_total_mib;
  const dirty = Object.keys(limits).some(
    (key) => policy[key as Field] !== settings.policy[key as Field],
  );
  const field = (key: Field) => (
    <label key={key} className="log-setting-field">
      <span>{t(`logging.${key}`)}</span>
      <Input
        type="number"
        min={1}
        max={limits[key]}
        step={1}
        required
        disabled={busy}
        value={values[key]}
        onChange={(e) =>
          setValues((previous) => ({ ...previous, [key]: e.target.value }))
        }
      />
    </label>
  );
  return (
    <form
      className="log-policy-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (fieldsValid && ordered && dirty && !busy) onSave(policy);
      }}
    >
      {normal.map(field)}
      <Collapsible open={expanded} onOpenChange={setExpanded}>
        <CollapsibleTrigger className="log-advanced-trigger">
          {t("logging.advanced")}
          <ChevronDown aria-hidden="true" />
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="log-advanced-fields">{advanced.map(field)}</div>
        </CollapsibleContent>
      </Collapsible>
      {!fieldsValid ? (
        <p className="short" role="alert">
          {t("logging.invalidValues")}
        </p>
      ) : !ordered ? (
        <p className="short" role="alert">
          {t("logging.invalidOrder")}
        </p>
      ) : null}
      <p className="quiet log-policy-hint">{t("logging.cleanupHint")}</p>
      <div className="settings-actions">
        <Button
          type="submit"
          size="sm"
          disabled={busy || !fieldsValid || !ordered || !dirty}
        >
          {busy ? <Spinner aria-hidden="true" /> : null}
          {t("logging.save")}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy}
          onClick={onReset}
        >
          {t("logging.reset")}
        </Button>
      </div>
    </form>
  );
}

export function LogSettings({ onBack }: { onBack: () => void }) {
  useTranslation();
  const query = useQuery(
    {
      queryKey: settingsKey,
      queryFn: () => loggingSettings("show"),
      staleTime: 0,
    },
    queryClient,
  );
  const save = useMutation(
    {
      mutationFn: ({
        action,
        policy,
      }: {
        action: "set" | "reset";
        policy?: LogPolicy;
      }) => loggingSettings(action, policy),
      onSuccess: (data) => queryClient.setQueryData(settingsKey, data),
    },
    queryClient,
  );
  const settings = query.data;
  return (
    <section className="log-settings" aria-label={t("logging.title")}>
      <div className="log-settings-head">
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label={t("logging.back")}
          disabled={save.isPending}
          onClick={onBack}
        >
          <ArrowLeft aria-hidden="true" />
        </Button>
        <h2>{t("logging.title")}</h2>
      </div>
      {query.isPending ? (
        <p role="status">
          <Spinner aria-hidden="true" /> {t("logging.loading")}
        </p>
      ) : null}
      {query.error ? (
        <>
          <FailureNote failure={failureOf(query.error)} />
          <Button
            variant="outline"
            size="sm"
            onClick={() => void query.refetch()}
          >
            {t("logging.retry")}
          </Button>
          {!settings ? (
            <Button
              variant="outline"
              size="sm"
              disabled={save.isPending}
              onClick={() => save.mutate({ action: "reset" })}
            >
              {t("logging.reset")}
            </Button>
          ) : null}
        </>
      ) : null}
      {settings ? (
        <>
          <dl className="log-usage">
            <div>
              <dt>{t("logging.sharedUsage")}</dt>
              <dd>
                {t("logging.usage", {
                  mib: (settings.usage.bytes / 1048576).toFixed(1),
                  count: settings.usage.files,
                })}
              </dd>
            </div>
            <div>
              <dt>{t("logging.daemonUsage")}</dt>
              <dd>
                {t("logging.usage", {
                  mib: (settings.daemon_usage.bytes / 1048576).toFixed(1),
                  count: settings.daemon_usage.files,
                })}
              </dd>
            </div>
          </dl>
          <PolicyForm
            key={JSON.stringify(settings.policy)}
            settings={settings}
            busy={save.isPending}
            onSave={(policy) => save.mutate({ action: "set", policy })}
            onReset={() => save.mutate({ action: "reset" })}
          />
          <dl className="log-paths">
            <div>
              <dt>{t("logging.folder")}</dt>
              <dd>{settings.directory}</dd>
            </div>
            <div>
              <dt>{t("logging.daemonFolder")}</dt>
              <dd>{settings.daemon_directory}</dd>
            </div>
          </dl>
        </>
      ) : null}
      {save.error ? <FailureNote failure={failureOf(save.error)} /> : null}
      {save.isSuccess ? (
        <p className="quiet" role="status">
          {t(
            save.variables.action === "reset"
              ? "logging.restored"
              : "logging.saved",
          )}
        </p>
      ) : null}
    </section>
  );
}
