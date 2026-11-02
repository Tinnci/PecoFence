import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import zhCN from "./locales/zh-CN.json";
import en from "./locales/en.json";

export type LanguagePreference = "system" | "zh-CN" | "en";
const storageKey = "spm.language.v1";
function readPreference(): LanguagePreference {
  try {
    const value = window.localStorage.getItem(storageKey);
    if (value === "zh-CN" || value === "en") return value;
  } catch {
    /* Preferences are optional when storage is unavailable. */
  }
  return "system";
}
let preference = readPreference();
export function resolveLanguage(
  value: LanguagePreference,
  languages: readonly string[] = navigator.languages,
): "zh-CN" | "en" {
  if (value !== "system") return value;
  for (const language of languages) {
    if (/^zh(?:-|$)/i.test(language)) return "zh-CN";
    if (/^en(?:-|$)/i.test(language)) return "en";
  }
  return "en";
}
export const i18n = i18next.createInstance();
void i18n.use(initReactI18next).init({
  resources: { "zh-CN": { translation: zhCN }, en: { translation: en } },
  lng: resolveLanguage(preference),
  fallbackLng: "en",
  supportedLngs: ["zh-CN", "en"],
  initAsync: false,
  keySeparator: false,
  interpolation: { escapeValue: false },
  react: { useSuspense: false },
});
export const t = (key: string, values: Record<string, unknown> = {}): string =>
  i18n.t(key, values);
export const locale = () => i18n.resolvedLanguage ?? "en";
export const languagePreference = () => preference;
export async function setLanguage(value: LanguagePreference) {
  preference = value;
  try {
    window.localStorage.setItem(storageKey, value);
  } catch {
    /* Keep the in-memory preference. */
  }
  await i18n.changeLanguage(resolveLanguage(value));
}
function updateDocument() {
  document.documentElement.lang = locale();
  document.documentElement.dir = i18n.dir();
}
if (typeof document !== "undefined") {
  updateDocument();
  i18n.on("languageChanged", updateDocument);
  window.addEventListener("languagechange", () => {
    if (preference === "system")
      void i18n.changeLanguage(resolveLanguage(preference));
  });
}
