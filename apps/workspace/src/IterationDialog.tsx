import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { iterationProblem, type IterationDates } from "./flow";
import { t } from "./i18n";
import { FailureNote, failureOf } from "./TemplateImport";
import type { SetupFailure } from "./contracts/setup";

/** Adds or edits one iteration; dates may not overlap another, as in spm. */
export function IterationDialog({
  editing,
  others,
  onSave,
  onClose,
}: {
  /** The iteration being edited; absent when adding one. */
  editing?: IterationDates;
  others: IterationDates[];
  onSave: (iteration: IterationDates | null) => Promise<void>;
  onClose: () => void;
}) {
  useTranslation();
  const [draft, setDraft] = useState<IterationDates>(
    editing ?? { name: "", start: "", end: "" },
  );
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const problem = iterationProblem(draft, others);
  const incomplete = !draft.name.trim() || !draft.start || !draft.end;
  async function commit(iteration: IterationDates | null) {
    setBusy(true);
    setFailure(null);
    try {
      await onSave(iteration);
      onClose();
    } catch (error) {
      setFailure(failureOf(error));
      setBusy(false);
    }
  }
  const field = (key: keyof IterationDates, label: string, type: string) => (
    <label className="release-field">
      <span>{label}</span>
      <Input
        type={type}
        value={draft[key]}
        maxLength={60}
        disabled={busy}
        onChange={(e) => setDraft({ ...draft, [key]: e.target.value })}
      />
    </label>
  );
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent className="local-dialog">
        <DialogTitle>
          {editing ? t("iteration.edit") : t("iteration.add")}
        </DialogTitle>
        <DialogDescription>{t("iteration.hint")}</DialogDescription>
        <form
          className="iteration-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (!problem && !busy)
              void commit({ ...draft, name: draft.name.trim() });
          }}
        >
          {field("name", t("iteration.name"), "text")}
          {field("start", t("iteration.start"), "date")}
          {field("end", t("iteration.end"), "date")}
          {problem && !incomplete ? (
            <p className="short" role="alert">
              {t(`iteration.problem.${problem}`)}
            </p>
          ) : null}
          {failure ? <FailureNote failure={failure} /> : null}
          <DialogFooter>
            {editing ? (
              <Button
                type="button"
                variant="ghost"
                disabled={busy}
                onClick={() => void commit(null)}
              >
                {t("iteration.delete")}
              </Button>
            ) : null}
            <Button
              type="button"
              variant="ghost"
              disabled={busy}
              onClick={onClose}
            >
              {t("model.cancel")}
            </Button>
            <Button type="submit" disabled={busy || !!problem}>
              {busy ? <Spinner aria-hidden="true" /> : null}
              {t("views.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
