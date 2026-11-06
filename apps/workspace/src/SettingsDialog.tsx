import { useTranslation } from "react-i18next";
import { useState } from "react";
import { LogSettings } from "./LogSettings";
import { Download, FolderPlus, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  languagePreference,
  setLanguage,
  t,
  type LanguagePreference,
} from "./i18n";
import type { Theme } from "./theme";

const languages: [LanguagePreference, () => string][] = [
  ["system", () => t("language.system")],
  ["zh-CN", () => "简体中文"],
  ["en", () => "English"],
];
const themes: Theme[] = ["light", "dark"];

/** Everything set once and left alone. */
export function SettingsDialog({
  open,
  version,
  theme,
  onOpenChange,
  onTheme,
  onBackup,
  onNewProject,
}: {
  open: boolean;
  version: string;
  theme: Theme;
  onOpenChange: (open: boolean) => void;
  onTheme: (theme: Theme) => void;
  onBackup: (mode: "export" | "restore") => void;
  onNewProject: () => void;
}) {
  useTranslation();
  const [logging, setLogging] = useState(false);
  const leave = (then: () => void) => {
    onOpenChange(false);
    then();
  };
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) setLogging(false);
        onOpenChange(next);
      }}
    >
      <DialogContent className="settings-dialog">
        <DialogTitle>{t("nav.settings")}</DialogTitle>
        {logging ? (
          <LogSettings onBack={() => setLogging(false)} />
        ) : (
          <>
            <div className="settings-row">
              <span>{t("language.label")}</span>
              <ToggleGroup
                variant="outline"
                size="sm"
                className="chips"
                aria-label={t("language.label")}
                value={[languagePreference()]}
                onValueChange={(next: string[]) => {
                  if (next[0]) void setLanguage(next[0] as LanguagePreference);
                }}
              >
                {languages.map(([value, label]) => (
                  <ToggleGroupItem key={value} value={value}>
                    {label()}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </div>
            <div className="settings-row">
              <span>{t("theme.label")}</span>
              <ToggleGroup
                variant="outline"
                size="sm"
                className="chips"
                aria-label={t("theme.label")}
                value={[theme]}
                onValueChange={(next: string[]) => {
                  if (next[0]) onTheme(next[0] as Theme);
                }}
              >
                {themes.map((value) => (
                  <ToggleGroupItem key={value} value={value}>
                    {t(`theme.${value}`)}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </div>
            <div className="settings-row">
              <span>{t("nav.data")}</span>
              <div className="settings-actions">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => leave(() => onBackup("export"))}
                >
                  <Download aria-hidden="true" />
                  {t("backup.export")}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => leave(() => onBackup("restore"))}
                >
                  <Upload aria-hidden="true" />
                  {t("backup.restore")}
                </Button>
              </div>
            </div>
            <div className="settings-row">
              <span>{t("nav.projects")}</span>
              <Button
                variant="outline"
                size="sm"
                onClick={() => leave(onNewProject)}
              >
                <FolderPlus aria-hidden="true" />
                {t("app.newSPMProject")}
              </Button>
            </div>
            <div className="settings-row">
              <span>{t("logging.logs")}</span>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setLogging(true)}
              >
                {t("logging.title")}
              </Button>
            </div>
            {version ? <p className="quiet menu-version">{version}</p> : null}
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
