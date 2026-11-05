// Runs before the app bundle: sets the saved theme so the splash matches it, then shows the
// window, so nothing paints white first. CSP allows scripts from 'self' only.
try {
  if (localStorage.getItem("tinnci.theme") === "dark")
    document.documentElement.classList.add("dark");
} catch {
  // Blocked storage leaves the default theme.
}
window.__TAURI_INTERNALS__?.invoke("show_main").catch(() => {});
