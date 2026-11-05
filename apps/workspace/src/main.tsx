import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "./focus-cache";
import { createRoot } from "react-dom/client";
import { isTauri, invoke } from "@tauri-apps/api/core";
import App from "./App";
import { MotionRoot } from "./components/motion";
import "./styles.css";
function reportError(kind: string, message: unknown) {
  if (isTauri())
    void invoke("client_error", { kind, message: String(message) }).catch(
      () => {},
    );
}
window.addEventListener("error", (e) => reportError("error", e.message));
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
