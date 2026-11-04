import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "./components/ui/button";
import { Input } from "./components/ui/input";
import { Spinner } from "./components/ui/spinner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "./components/ui/dialog";
import type { FocusModel } from "./contracts/setup";
import { t } from "./i18n";

export function RenameModelDialog({
  model,
  onSave,
  onClose,
}: {
  model: FocusModel;
  onSave: (name: string) => Promise<void>;
  onClose: () => void;
}) {
  useTranslation();
  const [name, setName] = useState(model.name ?? model.code);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const value = name.trim();
  const invalid = !value || /[\u0000-\u001f\u007f]/.test(value);
  async function save() {
    if (busy || invalid) return;
    setBusy(true);
    setFailed(false);
    try {
      await onSave(value);
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
        <DialogTitle>{t("model.rename")}</DialogTitle>
        <DialogDescription>
          {t("model.renameHint", { code: model.code })}
        </DialogDescription>
        <form
          className="local-row"
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <Input
            autoFocus
            aria-label={t("model.displayName")}
            value={name}
            maxLength={80}
            disabled={busy}
            onChange={(event) => setName(event.target.value)}
          />
          <Button type="submit" disabled={busy || invalid}>
            {busy ? <Spinner aria-hidden="true" /> : null}
            {t("views.save")}
          </Button>
        </form>
        <div className="local-row">
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => setName(model.code)}
          >
            {t("model.resetName")}
          </Button>
          <Button variant="outline" disabled={busy} onClick={onClose}>
            {t("model.cancelRename")}
          </Button>
        </div>
        {failed ? (
          <p className="short" role="alert">
            {t("model.renameFailed")}
          </p>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
