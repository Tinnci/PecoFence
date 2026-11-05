// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", () => mocks);
import { Platforms, ModelPlatform, platformsKey } from "../src/Platforms";
import { queryClient } from "../src/focus-cache";
import type { Platform } from "../src/contracts/setup";
const platform: Platform = {
  code: "P6700LV",
  vendor: "Unisoc",
  chip: "UMS9621S",
  baseband: "",
  modem: ["4G_MODEM_22B_W26.02.5_P2"],
  modem_repos: ["modem-branch"],
  modem_changes: 3,
  android: ["A16"],
  kernel: ["5.15"],
  evidence: [],
};
beforeEach(() => {
  queryClient.clear();
  mocks.focus.mockReset();
  mocks.focus.mockResolvedValue({ platforms: [platform] });
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});
it("saves only the edited baseband and refreshes the shared model header", async () => {
  queryClient.setQueryData(platformsKey, [platform]);
  mocks.focus.mockImplementation(async (action, args) => {
    if (action === "chip") return {};
    return { platforms: [{ ...platform, baseband: "MOLY.LR12A" }] };
  });
  render(
    <>
      <Platforms models={null} />
      <ModelPlatform code="P6700LV" />
    </>,
  );
  const field = screen.getByRole("textbox", { name: "P6700LV 的基带版本" });
  fireEvent.change(field, { target: { value: " MOLY.LR12A " } });
  fireEvent.blur(field);
  await waitFor(() =>
    expect(mocks.focus).toHaveBeenCalledWith("chip", {
      code: "P6700LV",
      baseband: "MOLY.LR12A",
    }),
  );
  await screen.findByText(/基带 MOLY.LR12A/);
  expect(screen.getByText(/4G_MODEM_22B/)).toBeTruthy();
  expect(screen.getByText(/modem-branch/)).toBeTruthy();
});
it("keeps a failed edit available for retry and supports clearing a value", async () => {
  queryClient.setQueryData(platformsKey, [{ ...platform, baseband: "OLD" }]);
  mocks.focus.mockRejectedValue(new Error("offline"));
  render(<Platforms models={null} />);
  const field = screen.getByRole("textbox", { name: "P6700LV 的基带版本" });
  fireEvent.change(field, { target: { value: "" } });
  fireEvent.blur(field);
  await waitFor(() => expect(field.getAttribute("aria-invalid")).toBe("true"));
  expect(mocks.focus).toHaveBeenCalledWith("chip", {
    code: "P6700LV",
    baseband: "",
  });
  expect((field as HTMLInputElement).value).toBe("");
});
