export type Theme = "light" | "dark";

const storageKey = "tinnci.theme";

function stored(): Theme {
  try {
    return window.localStorage.getItem(storageKey) === "dark"
      ? "dark"
      : "light";
  } catch {
    return "light";
  }
}

let theme = stored();
const apply = () =>
  document.documentElement.classList.toggle("dark", theme === "dark");

export const themePreference = () => theme;

export function setTheme(value: Theme) {
  theme = value;
  if (document.startViewTransition) document.startViewTransition(apply);
  else apply();
  try {
    window.localStorage.setItem(storageKey, value);
  } catch {
    // The choice still holds for this session.
  }
}

apply();
