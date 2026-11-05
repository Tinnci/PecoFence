import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import { ActionIcon } from "./components/action-icon";
import { FailureNote, failureOf } from "./TemplateImport";
import type { SetupFailure } from "./contracts/setup";
import { t } from "./i18n";

export function RemoveModelDialog({
  name,
  title,
  description,
  onRemove,
  onClose,
}: {
  name: string;
  title?: string;
  description?: string;
  onRemove: () => Promise<void>;
  onClose: () => void;
}) {
  useTranslation();
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  async function remove() {
    setBusy(true);
    setFailure(null);
    try {
      await onRemove();
      onClose();
    } catch (error) {
      setFailure(failureOf(error));
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
      <DialogContent>
        <DialogTitle>{title ?? t("model.unfollow2", { v0: name })}</DialogTitle>
        <DialogDescription>
          {description ??
            t("model.itsRulesAcceptedChangesAndTrackingRecordsWillBe")}
        </DialogDescription>
        {failure ? <FailureNote failure={failure} /> : null}
        <DialogFooter>
          <DialogClose render={<Button variant="ghost" disabled={busy} />}>
            <ActionIcon name="close" />
            {t("model.cancel")}
          </DialogClose>
          <Button
            variant="destructive"
            disabled={busy}
            onClick={() => void remove()}
          >
            <ActionIcon name="remove" />
            {title ? t("projectSetup.remove") : t("model.unfollow")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
