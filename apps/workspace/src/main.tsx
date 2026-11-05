import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "./focus-cache";
import { createRoot } from "react-dom/client";
import { isTauri, invoke } from "@tauri-apps/api/core";
import App from "./App";
import { MotionRoot } from "./components/motion";
import "./styles.css";
// Only the error's type and place reach the log: its message can hold anything.
function reportError(kind: string, error: unknown, at = "") {
  const name = error instanceof Error ? error.name : "unknown";
  if (isTauri())
    void invoke("client_error", { kind, name, at }).catch(() => {});
}
window.addEventListener("error", (e) =>
  reportError(
    "error",
    e.error,
    `${e.filename.split("/").pop() ?? ""}:${e.lineno}`,
  ),
);
window.addEventListener("unhandledrejection", (e) =>
  reportError("rejection", e.reason),
);
createRoot(document.getElementById("root")!).render(
  <QueryClientProvider client={queryClient}>
    <MotionRoot>
      <App />
    </MotionRoot>
  </QueryClientProvider>,
);
requestAnimationFrame(() =>
  requestAnimationFrame(() => {
    performance.mark("workspace-first-frame");
    if (isTauri()) void invoke("first_frame").catch(() => {});
  }),
);
