// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RenameModelDialog } from "../src/RenameModelDialog";
import type { FocusModel } from "../src/contracts/setup";

afterEach(cleanup);
it("saves a trimmed display name while retaining the model identity", async () => {
  const model: FocusModel = {
    code: "MODEL_A16",
    branches: ["MP"],
    jobs: [],
    issues: [],
    rules: {},
    targets: { severe_pct: 95, overall_pct: 85, fatal_max: 0 },
    accepted: [],
    tracked: [],
  };
  const save = vi.fn(async () => {});
  const close = vi.fn();
  const user = userEvent.setup();
  render(<RenameModelDialog model={model} onSave={save} onClose={close} />);
  const input = screen.getByRole("textbox", { name: "项目名（机型名）" });
  await user.clear(input);
  await user.type(input, " P6700LV-A16 {Enter}");
  expect(save).toHaveBeenCalledWith("P6700LV-A16");
  expect(close).toHaveBeenCalledOnce();
  expect(model.code).toBe("MODEL_A16");
});
