import { createRoot } from "react-dom/client";
import { isTauri, invoke } from "@tauri-apps/api/core";
import App from "./App";
import { MotionRoot } from "./components/motion";
import "./styles.css";
createRoot(document.getElementById("root")!).render(
  <MotionRoot>
    <App />
  </MotionRoot>,
);
requestAnimationFrame(() =>
  requestAnimationFrame(() => {
    performance.mark("workspace-first-frame");
    if (isTauri()) void invoke("first_frame").catch(() => {});
  }),
);
