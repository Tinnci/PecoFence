import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import { Spinner } from "@/components/ui/spinner";
import { backupApply, backupExport, backupPlan } from "./api";
import type { BackupPlan, SetupFailure } from "./contracts/setup";
import { t } from "./i18n";
import { FailureNote, failureOf } from "./TemplateImport";

type Stage =
  | { name: "picking" }
  | { name: "plan"; plan: BackupPlan }
  | { name: "applying"; plan: BackupPlan }
  | { name: "done"; text: string }
  | { name: "failed"; failure: SetupFailure };

function today() {
  return new Date().toISOString().slice(0, 10).replaceAll("-", "");
}

/** Saves or restores the followed models and local notes; SPM touches the file, not the page. */
export function DataBackup({
  mode,
  onRestored,
  onClose,
}: {
  mode: "export" | "restore";
  onRestored: () => Promise<unknown>;
  onClose: () => void;
}) {
  useTranslation();
  const [stage, setStage] = useState<Stage>({ name: "picking" });
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void (async () => {
      try {
        if (mode === "export") {
          const data = await backupExport(`spm-backup-${today()}.json`);
          const backup = data?.backup;
          if (!backup) return onClose();
          setStage({
            name: "done",
            text: t("backup.saved", {
              models: backup.models,
              items: backup.items,
              path: backup.path,
            }),
          });
        } else {
          const plan = (await backupPlan())?.backup_plan;
          if (!plan) return onClose();
          setStage({ name: "plan", plan });
        }
      } catch (error) {
        setStage({ name: "failed", failure: failureOf(error) });
      }
    })();
  }, [mode, onClose]);
  async function apply(plan: BackupPlan) {
    setStage({ name: "applying", plan });
    try {
      const restored = (await backupApply()).backup_restored;
      await onRestored();
      setStage({
        name: "done",
        text: t("backup.restored", {
          models: restored?.models ?? 0,
          items: restored?.items ?? 0,
        }),
      });
    } catch (error) {
      setStage({ name: "failed", failure: failureOf(error) });
    }
  }
  if (stage.name === "picking") return null;
  const plan =
    stage.name === "plan" || stage.name === "applying" ? stage.plan : null;
  const busy = stage.name === "applying";
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent>
        <DialogTitle>
          {mode === "export" ? t("backup.export") : t("backup.restore")}
        </DialogTitle>
        {plan ? (
          <DialogDescription>
            {t("backup.plan", {
              at: new Date(plan.created_at).toLocaleString(),
              added: plan.added,
              replaced: plan.replaced,
              kept: plan.kept,
              items: plan.items,
            })}
          </DialogDescription>
        ) : null}
        {stage.name === "done" ? (
          <DialogDescription>{stage.text}</DialogDescription>
        ) : null}
        {stage.name === "failed" ? (
          <FailureNote failure={stage.failure} />
        ) : null}
        <DialogFooter>
          <Button variant="ghost" disabled={busy} onClick={onClose}>
            {t(plan ? "model.cancel" : "backup.close")}
          </Button>
          {plan ? (
            <Button disabled={busy} onClick={() => void apply(plan)}>
              {busy ? <Spinner aria-hidden="true" /> : null}
              {t("backup.apply")}
            </Button>
          ) : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
